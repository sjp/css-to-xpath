//! Translation from Servo's parsed selector representation to XPath.

pub mod error;
mod generic;
mod nth;
mod pseudo;
pub mod xpath_expr;

pub use error::Error;

use selectors::attr::{NamespaceConstraint, ParsedAttrSelectorOperation, ParsedCaseSensitivity};
use selectors::parser::{Combinator, Component, Selector};

use crate::parser::{self, SelectrsImpl};
use xpath_expr::{Condition, XPathExpr, is_safe_name};

/// Which translator family the pseudo-class overrides come from: generic
/// or HTML (both `html` and `xhtml` use the HTML overrides; only `html`
/// lowercases names).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Generic,
    Html,
}

/// One struct with a kind tag and lowercasing flags. Casing is applied
/// here in the translator, never via Servo's parser settings, so the
/// translator families differ only in these fields.
pub struct Translator {
    pub(crate) kind: Kind,
    pub(crate) lower_case_element_names: bool,
    pub(crate) lower_case_attribute_names: bool,
}

/// The namespace constraint on a type or attribute selector: none
/// written, any, explicitly none, or a specific prefix.
#[derive(Clone, Copy)]
enum NsConstraint<'a> {
    /// No namespace separator written (`e`, `[foo]`).
    None,
    /// `*|e`, `[*|foo]`: any namespace, including none.
    Any,
    /// `|e`, `[|foo]`: explicitly no namespace.
    ExplicitNone,
    /// `ns|e`, `[ns|foo]`: a specific prefix (identity-mapped, no URL).
    Prefix(&'a str),
}

impl Translator {
    pub fn new(kind: &str) -> Option<Self> {
        match kind {
            "generic" => Some(Translator {
                kind: Kind::Generic,
                lower_case_element_names: false,
                lower_case_attribute_names: false,
            }),
            "html" => Some(Translator {
                kind: Kind::Html,
                lower_case_element_names: true,
                lower_case_attribute_names: true,
            }),
            "xhtml" => Some(Translator {
                kind: Kind::Html,
                lower_case_element_names: false,
                lower_case_attribute_names: false,
            }),
            _ => None,
        }
    }

    /// Translate comma-separated selector groups, each prefixed, joined
    /// with " | ".
    pub fn css_to_xpath(&self, css: &str, prefix: &str) -> Result<String, Error> {
        let list = parser::parse(css)?;
        let mut parts: Vec<String> = Vec::new();
        for sel in list.slice() {
            parts.push(self.selector_to_xpath(sel, prefix)?);
        }
        Ok(parts.join(" | "))
    }

    /// Iteration bridge: Servo iterates compound selectors right-to-left
    /// (match order), but the XPath is built left-to-right. Collect
    /// Servo's sequences + combinators, then fold from the leftmost
    /// compound.
    fn selector_to_xpath(
        &self,
        selector: &Selector<SelectrsImpl>,
        prefix: &str,
    ) -> Result<String, Error> {
        let seqs = collect_seqs(selector);

        // :scope is the node the XPath is evaluated from. In the leftmost
        // compound it anchors the expression on the self:: axis, which
        // replaces the prefix (`:scope > a` is `self::*/a`, the context
        // node's `a` children). Anywhere else the context node would have
        // to be named from inside a predicate, which XPath 1.0 cannot do.
        let leftmost = seqs.len() - 1;
        for (compound, _) in &seqs[..leftmost] {
            if compound.iter().any(|c| matches!(c, Component::Scope)) {
                return Err(Error::Unsupported(
                    "the `:scope` pseudo-class outside the leftmost compound".into(),
                ));
            }
        }
        let scope_anchored = seqs[leftmost]
            .0
            .iter()
            .any(|c| matches!(c, Component::Scope));

        // Leftmost compound first, then fold rightwards.
        let mut xpath = if scope_anchored {
            let compound: Vec<&Component<SelectrsImpl>> = seqs[leftmost]
                .0
                .iter()
                .filter(|c| !matches!(c, Component::Scope))
                .copied()
                .collect();
            let mut xp = self.compound_to_xpath(&compound)?;
            xp.path = "self::".to_owned();
            xp
        } else {
            self.compound_to_xpath(&seqs[leftmost].0)?
        };
        for i in (0..leftmost).rev() {
            let combinator = seqs[i]
                .1
                .ok_or_else(|| Error::Unsupported("an unexpected selector structure".into()))?;
            let right = self.compound_to_xpath(&seqs[i].0)?;
            xpath = self.apply_combinator(combinator, xpath, &right)?;
        }

        let prefix = if scope_anchored { "" } else { prefix };
        Ok(format!("{prefix}{}", xpath.str()))
    }

    /// Translate one compound selector (a sequence of simple selectors).
    /// Element-ish components (namespace, type) always precede condition
    /// components in a valid compound; conditions are applied in source
    /// order.
    fn compound_to_xpath(
        &self,
        components: &[&Component<SelectrsImpl>],
    ) -> Result<XPathExpr, Error> {
        let mut ns = NsConstraint::None;
        let mut element: Option<&str> = None;
        let mut xpath: Option<XPathExpr> = None;

        for component in components {
            match component {
                Component::Namespace(prefix, _) if xpath.is_none() => {
                    ns = NsConstraint::Prefix(prefix.as_str());
                }
                // The sentinel default namespace (see SelectrsParser):
                // plain `e` and type-less compounds — no constraint written.
                Component::DefaultNamespace(_) if xpath.is_none() => {
                    ns = NsConstraint::None;
                }
                Component::ExplicitAnyNamespace if xpath.is_none() => {
                    ns = NsConstraint::Any;
                }
                Component::ExplicitNoNamespace if xpath.is_none() => {
                    ns = NsConstraint::ExplicitNone;
                }
                Component::ExplicitUniversalType if xpath.is_none() => {}
                Component::LocalName(local_name) if xpath.is_none() => {
                    element = Some(local_name.name.as_str());
                }
                other => {
                    let xp = match xpath {
                        Some(ref mut xp) => xp,
                        None => {
                            xpath = Some(self.xpath_element(ns, element));
                            xpath.as_mut().expect("just set")
                        }
                    };
                    self.apply_simple(xp, other)?;
                }
            }
        }

        Ok(match xpath {
            Some(xp) => xp,
            None => self.xpath_element(ns, element),
        })
    }

    /// Build the element part of the expression from the namespace
    /// constraint and element name.
    fn xpath_element(&self, ns: NsConstraint, element: Option<&str>) -> XPathExpr {
        let (mut name, mut safe) = match element {
            None => ("*".to_owned(), true),
            Some(e) => {
                let safe = is_safe_name(e);
                let e = if self.lower_case_element_names {
                    e.to_lowercase()
                } else {
                    e.to_owned()
                };
                (e, safe)
            }
        };
        match ns {
            NsConstraint::Any if name != "*" => {
                // '*|e': 'e' in any namespace, including none. An unprefixed
                // XPath name test only matches the null namespace, so test
                // against local-name() instead. The of-type nodetest counts
                // by local name too, an approximation: siblings sharing the
                // name across namespaces are distinct types per the spec,
                // but XPath 1.0 cannot compare a sibling's namespace
                // against the matched element's.
                let cond = format!("local-name() = {}", xpath_expr::xpath_literal(&name));
                let mut xpath = XPathExpr::new("*");
                xpath.name_test = Some(format!("*[{cond}]"));
                xpath.add_condition(&cond);
                return xpath;
            }
            NsConstraint::ExplicitNone if name == "*" || !safe => {
                // A safe '|e' is just an unprefixed XPath name test, which
                // matches exactly the null namespace. '|*' and names
                // needing quoting check namespace-uri() explicitly: a
                // quoted name() test alone would also match the name in a
                // default namespace.
                let mut xpath = XPathExpr::new(&name);
                xpath.add_name_test();
                xpath.add_condition("namespace-uri() = ''");
                if name != "*" {
                    // The of-type nodetest must carry the namespace pin
                    // set by the condition above
                    xpath.name_test = Some(format!(
                        "*[name() = {} and namespace-uri() = '']",
                        xpath_expr::xpath_literal(&name)
                    ));
                }
                return xpath;
            }
            NsConstraint::Prefix(prefix) => {
                // Namespace prefixes are case-sensitive.
                // https://www.w3.org/TR/css-namespaces-3/#prefixes
                safe = safe && is_safe_name(prefix);
                name = format!("{prefix}:{name}");
            }
            // '*|*' and '|e' translate to an unqualified name test.
            _ => {}
        }
        let mut xpath = XPathExpr::new(&name);
        if !safe {
            xpath.add_name_test();
        }
        xpath
    }

    /// Dispatch over the non-element components of a compound — the
    /// allow-list over `Component` variants. Anything outside the
    /// supported construct set errors, never approximates.
    fn apply_simple(
        &self,
        xpath: &mut XPathExpr,
        component: &Component<SelectrsImpl>,
    ) -> Result<(), Error> {
        match component {
            // :root
            Component::Root => {
                xpath.add_condition("not(parent::*)");
                Ok(())
            }
            // :empty
            Component::Empty => {
                xpath.add_condition("not(*) and not(string-length())");
                Ok(())
            }
            // :first-child, :nth-child(an+b), :only-of-type, ... — Servo
            // collapses the whole family into NthSelectorData.
            Component::Nth(data) => self.apply_nth(xpath, data, None),
            // :nth-child(an+b of S) / :nth-last-child(an+b of S)
            Component::NthOf(data) => {
                self.apply_nth(xpath, data.nth_data(), Some(data.selectors()))
            }
            // :not(). Nesting inside other functional pseudo-classes is
            // allowed (Selectors Level 4).
            Component::Negation(list) => {
                match self.arg_conditions(list.slice(), ":not()")? {
                    Some(conditions) if !conditions.is_empty() => {
                        // not(...) supplies its own grouping, so the
                        // or-join needs no parentheses.
                        let joined = Condition::join_or(&conditions);
                        xpath.add_condition(&format!("not({})", joined.expr));
                    }
                    // A universal argument makes the negation unmatchable.
                    _ => xpath.add_condition("0"),
                }
                Ok(())
            }
            // :is()/:matches() and :where() — identical translations: the
            // arguments OR together into a single condition that is AND-ed
            // onto the outer expression, keeping the compound a conjunction.
            Component::Is(list) | Component::Where(list) => {
                let context = match component {
                    Component::Is(_) => ":is()",
                    _ => ":where()",
                };
                // None means an argument matched everything, so the whole
                // pseudo-class is a no-op constraint.
                if let Some(conditions) = self.arg_conditions(list.slice(), context)?
                    && !conditions.is_empty()
                {
                    xpath.push_condition(Condition::join_or(&conditions));
                }
                Ok(())
            }
            // :has(): each argument is a relative selector whose optional
            // leading combinator scopes the match (`>` child, `~`
            // subsequent sibling, `+` next sibling; omitted means
            // descendant). Unlike the other functional pseudo-classes,
            // :has() looks forward, so a complex argument extends the
            // existence-test path step by step, leftmost compound first.
            Component::Has(relatives) => {
                let mut conditions: Vec<String> = Vec::new();
                for relative in relatives.iter() {
                    let seqs = collect_seqs(&relative.selector);
                    // The leftmost sequence is the anchor (the candidate
                    // element itself); its combinator slot carries the
                    // argument's leading combinator.
                    let anchor = &seqs[seqs.len() - 1].0;
                    let anchor_only = seqs.len() >= 2
                        && anchor.len() == 1
                        && matches!(anchor[0], Component::RelativeSelectorAnchor);
                    if !anchor_only {
                        return Err(Error::Unsupported(
                            "an unexpected selector structure inside `:has()`".into(),
                        ));
                    }
                    let mut test = String::new();
                    for i in (0..seqs.len() - 1).rev() {
                        let first = i == seqs.len() - 2;
                        let combinator = seqs[i].1;
                        // The first step is an axis from the candidate
                        // element; later steps join onto the path.
                        let axis = match (first, combinator) {
                            (true, Some(Combinator::Descendant)) => ".//",
                            (true, Some(Combinator::Child)) => "child::",
                            (
                                true,
                                Some(Combinator::NextSibling) | Some(Combinator::LaterSibling),
                            ) => "following-sibling::",
                            (false, Some(Combinator::Descendant)) => "//",
                            (false, Some(Combinator::Child)) => "/",
                            (
                                false,
                                Some(Combinator::NextSibling) | Some(Combinator::LaterSibling),
                            ) => "/following-sibling::",
                            (_, other) => {
                                return Err(Error::Unsupported(format!(
                                    "an unexpected combinator ({other:?}) inside `:has()`"
                                )));
                            }
                        };
                        let mut sub = self.compound_to_xpath(&seqs[i].0)?;
                        // A prefixed name stays in the node test
                        // (`.//svg:g`) so it resolves through the
                        // namespace map, except under `+` where the [1]
                        // position predicate needs the node test to
                        // stay `*`.
                        if !sub.element.contains(':') {
                            sub.add_name_test();
                        } else if matches!(combinator, Some(Combinator::NextSibling)) {
                            let element = std::mem::replace(&mut sub.element, "*".to_owned());
                            sub.add_condition(&format!("self::{element}"));
                        }
                        if matches!(combinator, Some(Combinator::NextSibling)) {
                            // Only the immediately following sibling:
                            // constrain position before applying the match
                            // conditions.
                            sub.add_predicate("1");
                        }
                        test.push_str(axis);
                        test.push_str(&sub.str());
                    }
                    conditions.push(test);
                }
                if !conditions.is_empty() {
                    xpath.add_condition(&conditions.join(" | "));
                }
                Ok(())
            }
            // :hover, :checked, :lang(), ... — translator-dependent.
            Component::NonTSPseudoClass(pc) => self.apply_pseudo_class(xpath, pc),
            // e#myid
            Component::ID(id) => {
                self.attrib_equals(xpath, "@id", id.as_str());
                Ok(())
            }
            // .foo is defined as [class~=foo] in the spec
            Component::Class(class_name) => {
                self.attrib_includes(xpath, "@class", class_name.as_str());
                Ok(())
            }
            Component::AttributeInNoNamespaceExists { local_name, .. } => {
                let attrib = self.attrib_expr(NsConstraint::None, local_name.as_str());
                xpath.add_condition(&attrib);
                Ok(())
            }
            Component::AttributeInNoNamespace {
                local_name,
                operator,
                value,
                case_sensitivity,
            } => {
                let attrib = self.attrib_expr(NsConstraint::None, local_name.as_str());
                let (attrib, value) = apply_case_flag(attrib, value.as_str(), case_sensitivity);
                self.attrib_operator(xpath, &attrib, *operator, &value)
            }
            Component::AttributeOther(attr) => {
                let ns = match attr.namespace {
                    Some(NamespaceConstraint::Specific((ref prefix, _))) => {
                        NsConstraint::Prefix(prefix.as_str())
                    }
                    Some(NamespaceConstraint::Any) => NsConstraint::Any,
                    // '[|foo]' is equivalent to '[foo]': unprefixed
                    // attribute names have no namespace.
                    None => NsConstraint::None,
                };
                let attrib = self.attrib_expr(ns, attr.local_name.as_str());
                match attr.operation {
                    ParsedAttrSelectorOperation::Exists => {
                        xpath.add_condition(&attrib);
                        Ok(())
                    }
                    ParsedAttrSelectorOperation::WithValue {
                        operator,
                        case_sensitivity,
                        ref value,
                    } => {
                        let (attrib, value) =
                            apply_case_flag(attrib, value.as_str(), &case_sensitivity);
                        self.attrib_operator(xpath, &attrib, operator, &value)
                    }
                }
            }
            unsupported => Err(Error::Unsupported(describe_component(unsupported))),
        }
    }

    /// Attribute-name handling: lowercase (html), safety check, namespace
    /// qualification (note: a specific namespace prefix is not part of the
    /// safety check).
    fn attrib_expr(&self, ns: NsConstraint, local_name: &str) -> String {
        let name = if self.lower_case_attribute_names {
            local_name.to_lowercase()
        } else {
            local_name.to_owned()
        };
        let safe = is_safe_name(&name);
        match ns {
            NsConstraint::Any => {
                // '[*|attr]': 'attr' in any namespace, including none. An
                // unprefixed XPath attribute test only matches attributes
                // with no namespace, so test against local-name() instead.
                format!("@*[local-name() = {}]", xpath_expr::xpath_literal(&name))
            }
            NsConstraint::Prefix(prefix) => {
                let name = format!("{prefix}:{name}");
                if safe {
                    format!("@{name}")
                } else {
                    format!(
                        "attribute::*[name() = {}]",
                        xpath_expr::xpath_literal(&name)
                    )
                }
            }
            NsConstraint::None | NsConstraint::ExplicitNone => {
                if safe {
                    format!("@{name}")
                } else {
                    format!(
                        "attribute::*[name() = {}]",
                        xpath_expr::xpath_literal(&name)
                    )
                }
            }
        }
    }

    /// Join two compound translations with a combinator.
    fn apply_combinator(
        &self,
        combinator: Combinator,
        mut left: XPathExpr,
        right: &XPathExpr,
    ) -> Result<XPathExpr, Error> {
        match combinator {
            Combinator::Descendant => left.join("//", right),
            Combinator::Child => left.join("/", right),
            Combinator::LaterSibling => left.join("/following-sibling::", right),
            Combinator::NextSibling => {
                left.join("/following-sibling::", right);
                // The node test moves into a self:: predicate so the [1]
                // position test counts every sibling, not only same-name
                // ones: *[1][self::element][existing conditions].
                let target_element = std::mem::replace(&mut left.element, "*".to_owned());
                left.add_predicate("1");
                left.add_predicate(&format!("self::{target_element}"));
            }
            // PseudoElement / SlotAssignment / Part combinators can never be
            // produced: the corresponding parser hooks are disabled.
            other => {
                return Err(Error::Unsupported(format!("the {other:?} combinator")));
            }
        }
        Ok(left)
    }

    /// Harvest the conditions of a pseudo-class argument list, the shared
    /// pattern of :not()/:is()/:where() and the nth `of S` handling:
    /// translate each argument into a condition on the candidate element.
    ///
    /// Returns `None` when any argument matches everything (e.g. `*`): the
    /// OR of the list is then trivially true, so callers must not constrain
    /// on the remaining arguments.
    fn arg_conditions(
        &self,
        selectors: &[Selector<SelectrsImpl>],
        context: &str,
    ) -> Result<Option<Vec<Condition>>, Error> {
        let mut conditions = Vec::new();
        let mut trivially_true = false;
        for selector in selectors {
            let seqs = collect_seqs(selector);
            match self.argument_condition(&seqs, 0, context)? {
                None => trivially_true = true,
                Some(condition) => conditions.push(condition),
            }
        }
        Ok(if trivially_true {
            None
        } else {
            Some(conditions)
        })
    }

    /// The condition imposed on the candidate element by the argument
    /// chain from `seqs[idx]` leftwards. The compound's element becomes a
    /// condition — a `self::` node test for prefixed names (so the prefix
    /// resolves through the namespace map, like a top-level `svg|g`), a
    /// `name()` comparison otherwise. A complex argument applies its
    /// rightmost compound to the candidate, with everything to its left
    /// becoming an existence test through reversed axes, recursively:
    /// `:is(a > b ~ c)` matches a `c` with a preceding sibling `b` whose
    /// parent is an `a`.
    ///
    /// `None` means the chain imposes no condition (a bare `*` argument).
    fn argument_condition(
        &self,
        seqs: &[(Vec<&Component<SelectrsImpl>>, Option<Combinator>)],
        idx: usize,
        context: &str,
    ) -> Result<Option<Condition>, Error> {
        let (compound, combinator) = &seqs[idx];
        let mut sub = self.compound_to_xpath(compound)?;
        if sub.element.contains(':') {
            let element = std::mem::replace(&mut sub.element, "*".to_owned());
            sub.add_condition(&format!("self::{element}"));
        } else {
            sub.add_name_test();
        }
        if idx + 1 < seqs.len() {
            // The axis pointing back at where the left-hand side of the
            // combinator must be, relative to the element matched here.
            let axis = match combinator {
                Some(Combinator::Descendant) => "ancestor::*",
                Some(Combinator::Child) => "parent::*",
                Some(Combinator::LaterSibling) => "preceding-sibling::*",
                Some(Combinator::NextSibling) => "preceding-sibling::*[1]",
                other => {
                    return Err(Error::Unsupported(format!(
                        "an unexpected combinator ({other:?}) inside `{context}`"
                    )));
                }
            };
            let rev_test = match self.argument_condition(seqs, idx + 1, context)? {
                Some(inner) => format!("{axis}[{}]", inner.expr),
                None => axis.to_owned(),
            };
            sub.add_condition(&rev_test);
        }
        Ok(sub.condition())
    }
}

/// Collect a selector's compound sequences in match order: `seqs[i]` is
/// (compound, combinator between this compound and the one to its left),
/// so `seqs[0]` is the rightmost compound and only the last entry's
/// combinator is `None`.
fn collect_seqs(
    selector: &Selector<SelectrsImpl>,
) -> Vec<(Vec<&Component<SelectrsImpl>>, Option<Combinator>)> {
    let mut iter = selector.iter();
    let mut seqs: Vec<(Vec<&Component<SelectrsImpl>>, Option<Combinator>)> = Vec::new();
    loop {
        let compound: Vec<&Component<SelectrsImpl>> = (&mut iter).collect();
        let combinator = iter.next_sequence();
        let done = combinator.is_none();
        seqs.push((compound, combinator));
        if done {
            break;
        }
    }
    seqs
}

/// The Level 4 case-sensitivity flag handling.
///
/// `[attr="value" i]`: compare the ASCII-lowercased attribute (via XPath
/// `translate()`) against the ASCII-lowercased value. An empty value needs
/// no lowercasing, and skipping it keeps the existence tests exact. The `s`
/// flag, the no-flag default, and Servo's HTML-legacy-attribute default all
/// mean the ordinary case-sensitive translation.
fn apply_case_flag(
    attrib: String,
    value: &str,
    case_sensitivity: &ParsedCaseSensitivity,
) -> (String, String) {
    match case_sensitivity {
        ParsedCaseSensitivity::AsciiCaseInsensitive if !value.is_empty() => (
            format!(
                "translate({attrib}, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                 'abcdefghijklmnopqrstuvwxyz')"
            ),
            value.to_ascii_lowercase(),
        ),
        _ => (attrib, value.to_owned()),
    }
}

/// Human-readable construct names for unsupported-error messages.
fn describe_component(component: &Component<SelectrsImpl>) -> String {
    match component {
        // Top-level :scope is handled (or rejected) in selector_to_xpath,
        // so reaching this arm means :scope sits inside a functional
        // pseudo-class argument, where the context node is unreachable.
        Component::Scope | Component::ImplicitScope => {
            "the `:scope` pseudo-class inside a functional pseudo-class".into()
        }
        Component::Slotted(..) => "the `::slotted()` pseudo-element".into(),
        Component::Part(..) => "the `::part()` pseudo-element".into(),
        Component::Host(..) => "the `:host` pseudo-class".into(),
        Component::ParentSelector => "the `&` parent selector".into(),
        // PseudoElement carries an uninhabited type and the remaining
        // variants require parser features selectrs never enables; they are
        // unreachable, but erroring beats panicking (panic = abort would
        // terminate the R session).
        other => format!("an unexpected construct ({other:?})"),
    }
}
