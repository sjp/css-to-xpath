//! Translation from Servo's parsed selector representation to XPath.

pub mod error;
mod generic;
mod nth;
mod pseudo;
pub mod xpath_expr;

pub use error::Error;

use selectors::attr::{NamespaceConstraint, ParsedAttrSelectorOperation, ParsedCaseSensitivity};
use selectors::parser::{Combinator, Component, Selector};

use crate::parser::{self, CssToXpathImpl};
use pseudo::LangSource;
use xpath_expr::{Condition, XPathExpr, is_safe_name};

/// Which translator family the pseudo-class overrides come from: generic
/// or HTML (both `html` and `xhtml` use the HTML overrides; they differ
/// in name casing and in the `:lang()` language source).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Generic,
    Html,
}

/// The translator flavour: which pseudo-class overrides, name-casing
/// rules, and `:lang()` language source to apply. `Html` and `Xhtml`
/// share the HTML overrides; only `Html` ASCII-lowercases element and attribute
/// names, and only `Xhtml` reads `xml:lang`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Generic,
    Html,
    Xhtml,
}

/// One struct with a kind tag, lowercasing flags, and the `:lang()`
/// language source. Casing is applied here in the translator, never via
/// Servo's parser settings, so the translator families differ only in
/// these fields.
pub struct Translator {
    pub(crate) kind: Kind,
    pub(crate) lower_case_element_names: bool,
    pub(crate) lower_case_attribute_names: bool,
    /// Whether the target document is an HTML document, which is what
    /// makes HTML's legacy case-insensitive attribute values fold (see
    /// `apply_case_flag`). Only `Mode::Html` sets it: `Mode::Xhtml` is
    /// XML, where those attributes compare case-sensitively. It is kept
    /// apart from `lower_case_attribute_names` because the two answer
    /// different questions — how a name is spelled versus how a value is
    /// compared — even though today's modes happen to agree on both.
    pub(crate) html_document: bool,
    pub(crate) lang_source: LangSource,
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
    pub fn new(mode: Mode) -> Self {
        match mode {
            Mode::Generic => Translator {
                kind: Kind::Generic,
                lower_case_element_names: false,
                lower_case_attribute_names: false,
                html_document: false,
                lang_source: LangSource::XmlLang,
            },
            Mode::Html => Translator {
                kind: Kind::Html,
                lower_case_element_names: true,
                lower_case_attribute_names: true,
                html_document: true,
                lang_source: LangSource::Lang,
            },
            Mode::Xhtml => Translator {
                kind: Kind::Html,
                lower_case_element_names: false,
                lower_case_attribute_names: false,
                html_document: false,
                lang_source: LangSource::Both,
            },
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
        selector: &Selector<CssToXpathImpl>,
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
            let compound: Vec<&Component<CssToXpathImpl>> = seqs[leftmost]
                .0
                .iter()
                .filter(|c| !matches!(c, Component::Scope))
                .copied()
                .collect();
            let mut xp = self.compound_to_xpath(&compound, 0)?;
            xp.path = "self::".to_owned();
            xp
        } else {
            self.compound_to_xpath(&seqs[leftmost].0, 0)?
        };
        for i in (0..leftmost).rev() {
            let combinator = seqs[i]
                .1
                .ok_or_else(|| Error::Unsupported("an unexpected selector structure".into()))?;
            let right = self.compound_to_xpath(&seqs[i].0, 0)?;
            xpath = self.apply_combinator(combinator, xpath, &right)?;
        }

        let prefix = if scope_anchored { "" } else { prefix };
        Ok(format!("{prefix}{}", xpath.str()))
    }

    /// Translate one compound selector (a sequence of simple selectors).
    /// Element-ish components (namespace, type) always precede condition
    /// components in a valid compound; conditions are applied in source
    /// order.
    ///
    /// `of_depth` is how many `An+B of S` argument lists this compound is
    /// nested inside; see [`nth::MAX_NTH_OF_DEPTH`].
    fn compound_to_xpath(
        &self,
        components: &[&Component<CssToXpathImpl>],
        of_depth: usize,
    ) -> Result<XPathExpr, Error> {
        let mut ns = NsConstraint::None;
        let mut element: Option<&str> = None;
        let mut xpath: Option<XPathExpr> = None;

        for component in components {
            match component {
                Component::Namespace(prefix, _) if xpath.is_none() => {
                    ns = NsConstraint::Prefix(prefix.as_str());
                }
                // The sentinel default namespace (see CssToXpathParser):
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
                            xpath = Some(self.xpath_element(ns, element)?);
                            xpath.as_mut().expect("just set")
                        }
                    };
                    self.apply_simple(xp, other, of_depth)?;
                }
            }
        }

        Ok(match xpath {
            Some(xp) => xp,
            None => self.xpath_element(ns, element)?,
        })
    }

    /// Build the element part of the expression from the namespace
    /// constraint and element name.
    fn xpath_element(&self, ns: NsConstraint, element: Option<&str>) -> Result<XPathExpr, Error> {
        let (mut name, safe) = match element {
            None => ("*".to_owned(), true),
            Some(e) => {
                let safe = is_safe_name(e);
                let e = if self.lower_case_element_names {
                    e.to_ascii_lowercase()
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
                return Ok(xpath);
            }
            NsConstraint::ExplicitNone if name == "*" => {
                // '|*': every element with no namespace. A bare '*' is
                // every element whatever its namespace, so the constraint
                // has to be written out.
                let mut xpath = XPathExpr::new("*");
                xpath.add_condition("namespace-uri() = ''");
                return Ok(xpath);
            }
            NsConstraint::None | NsConstraint::ExplicitNone if !safe => {
                // A safe 'e' or '|e' is just an unprefixed XPath name
                // test, which matches exactly the null namespace. A name
                // needing quoting cannot be a name test at all, so it
                // folds into a name() comparison — and name() returns the
                // *qualified* name, which for an element in a default
                // namespace is the bare local name. Pin namespace-uri()
                // alongside it so a quoted name matches exactly what a
                // safe one does.
                let cond = format!("name() = {}", xpath_expr::xpath_literal(&name));
                let mut xpath = XPathExpr::new("*");
                // The of-type nodetest must carry the namespace pin set
                // by the condition below.
                xpath.name_test = Some(format!("*[{cond} and namespace-uri() = '']"));
                xpath.add_condition(&cond);
                xpath.add_condition("namespace-uri() = ''");
                return Ok(xpath);
            }
            // Namespace prefixes are case-sensitive.
            // https://www.w3.org/TR/css-namespaces-3/#prefixes
            NsConstraint::Prefix(prefix) if !is_safe_name(prefix) => {
                return Err(unsafe_prefix_error(prefix));
            }
            NsConstraint::Prefix(prefix) if !safe => {
                // Only the local name needs quoting: keep the prefix in
                // the node test so the engine still resolves it through
                // the caller's namespace map, and compare the local part
                // alone. Folding the whole 'prefix:name' into a name()
                // test would instead match only documents that happen to
                // use that very prefix.
                let cond = format!("local-name() = {}", xpath_expr::xpath_literal(&name));
                let mut xpath = XPathExpr::new(&format!("{prefix}:*"));
                // The of-type nodetest must carry the local-name test set
                // by the condition below.
                xpath.name_test = Some(format!("{prefix}:*[{cond}]"));
                xpath.add_condition(&cond);
                return Ok(xpath);
            }
            NsConstraint::Prefix(prefix) => {
                name = format!("{prefix}:{name}");
            }
            // 'e', '|e' and '*|*' translate to an unqualified name test.
            _ => {}
        }
        // Every name needing quoting was handled above, so what is left
        // is a plain node test: '*', 'e', 'ns:e' or 'ns:*'.
        Ok(XPathExpr::new(&name))
    }

    /// Dispatch over the non-element components of a compound — the
    /// allow-list over `Component` variants. Anything outside the
    /// supported construct set errors, never approximates.
    fn apply_simple(
        &self,
        xpath: &mut XPathExpr,
        component: &Component<CssToXpathImpl>,
        of_depth: usize,
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
            Component::Nth(data) => self.apply_nth(xpath, data, None, of_depth),
            // :nth-child(an+b of S) / :nth-last-child(an+b of S)
            Component::NthOf(data) => {
                self.apply_nth(xpath, data.nth_data(), Some(data.selectors()), of_depth)
            }
            // :not(). Nesting inside other functional pseudo-classes is
            // allowed (Selectors Level 4).
            Component::Negation(list) => {
                match self.arg_conditions(list.slice(), ":not()", of_depth)? {
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
                // Selectors 4 makes these argument lists forgiving, so an
                // empty one is valid and matches nothing. The parser
                // accepts that recovery and no other, so any other list
                // here is an ordinary one.
                if parser::is_empty_forgiving_list(list.slice()) {
                    xpath.add_condition("0");
                    return Ok(());
                }
                let context = match component {
                    Component::Is(_) => ":is()",
                    _ => ":where()",
                };
                // None means an argument matched everything, so the whole
                // pseudo-class is a no-op constraint.
                if let Some(conditions) = self.arg_conditions(list.slice(), context, of_depth)?
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
                        let mut sub = self.compound_to_xpath(&seqs[i].0, of_depth)?;
                        // The name stays in the node test (`.//p`,
                        // `.//svg:g`) so it means exactly what it means at
                        // the top level and a prefix resolves through the
                        // namespace map — except under `+`, where the [1]
                        // position predicate has to count every sibling,
                        // so the node test must stay `*`.
                        if matches!(combinator, Some(Combinator::NextSibling)) {
                            sub.take_element_into_self_test();
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
                let attrib = self.attrib_expr(NsConstraint::None, local_name.as_str())?;
                xpath.add_condition(&attrib);
                Ok(())
            }
            Component::AttributeInNoNamespace {
                local_name,
                operator,
                value,
                case_sensitivity,
            } => {
                let attrib = self.attrib_expr(NsConstraint::None, local_name.as_str())?;
                let (attrib, value) =
                    self.apply_case_flag(attrib, value.as_str(), case_sensitivity);
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
                let attrib = self.attrib_expr(ns, attr.local_name.as_str())?;
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
                            self.apply_case_flag(attrib, value.as_str(), &case_sensitivity);
                        self.attrib_operator(xpath, &attrib, operator, &value)
                    }
                }
            }
            unsupported => Err(Error::Unsupported(describe_component(unsupported))),
        }
    }

    /// Whether an attribute-value comparison is case-sensitive, and the
    /// resulting comparison pair.
    ///
    /// Selectors 4 leaves attribute-value case sensitivity to the document
    /// language unless a flag overrides it, and HTML makes a fixed list of
    /// attributes (`type`, `rel`, `dir`, `checked`, ... — the presentational
    /// and enumerated legacy ones) ASCII case-insensitive on HTML elements in
    /// HTML documents. Servo's parser does that classification for us and
    /// hands back `AsciiCaseInsensitiveIfInHtmlElementInHtmlDocument`, already
    /// restricted to unflagged, un-namespaced attributes. The other half of
    /// that variant's condition — that the element is in the HTML namespace —
    /// is not checkable from a selector, but it holds wherever the flag does:
    /// an HTML parser puts every element in one document, without namespaces.
    ///
    /// Folding means comparing the ASCII-lowercased attribute (via XPath
    /// `translate()`) against the ASCII-lowercased value. An empty value needs
    /// no lowercasing, and skipping it keeps the existence tests exact.
    fn apply_case_flag(
        &self,
        attrib: String,
        value: &str,
        case_sensitivity: &ParsedCaseSensitivity,
    ) -> (String, String) {
        let fold = match case_sensitivity {
            // `[attr="value" i]`.
            ParsedCaseSensitivity::AsciiCaseInsensitive => true,
            // No flag on one of HTML's case-insensitive attributes: it
            // folds only where the document is HTML. `Mode::Xhtml` is XML,
            // where these attributes are case-sensitive like any other.
            ParsedCaseSensitivity::AsciiCaseInsensitiveIfInHtmlElementInHtmlDocument => {
                self.html_document
            }
            // `[attr="value" s]`, and the case-sensitive no-flag default.
            ParsedCaseSensitivity::ExplicitCaseSensitive | ParsedCaseSensitivity::CaseSensitive => {
                false
            }
        };
        if fold && !value.is_empty() {
            (xpath_expr::ascii_lower(&attrib), value.to_ascii_lowercase())
        } else {
            (attrib, value.to_owned())
        }
    }

    /// Attribute-name handling: ASCII-lowercase (html), safety check, namespace
    /// qualification. Prefixes take part in the safety check, as in
    /// `xpath_element`: one that needs quoting cannot be a node test at
    /// all, and errors.
    fn attrib_expr(&self, ns: NsConstraint, local_name: &str) -> Result<String, Error> {
        let name = if self.lower_case_attribute_names {
            local_name.to_ascii_lowercase()
        } else {
            local_name.to_owned()
        };
        let safe = is_safe_name(&name);
        match ns {
            NsConstraint::Any => {
                // '[*|attr]': 'attr' in any namespace, including none. An
                // unprefixed XPath attribute test only matches attributes
                // with no namespace, so test against local-name() instead.
                Ok(format!(
                    "@*[local-name() = {}]",
                    xpath_expr::xpath_literal(&name)
                ))
            }
            NsConstraint::Prefix(prefix) if !is_safe_name(prefix) => {
                Err(unsafe_prefix_error(prefix))
            }
            NsConstraint::Prefix(prefix) if !safe => {
                // As in `xpath_element`: the prefix stays in the node test
                // so it resolves through the caller's namespace map, and
                // only the local part is compared.
                Ok(format!(
                    "@{prefix}:*[local-name() = {}]",
                    xpath_expr::xpath_literal(&name)
                ))
            }
            NsConstraint::Prefix(prefix) => Ok(format!("@{prefix}:{name}")),
            NsConstraint::None | NsConstraint::ExplicitNone => Ok(if safe {
                format!("@{name}")
            } else {
                format!(
                    "attribute::*[name() = {}]",
                    xpath_expr::xpath_literal(&name)
                )
            }),
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
        selectors: &[Selector<CssToXpathImpl>],
        context: &str,
        of_depth: usize,
    ) -> Result<Option<Vec<Condition>>, Error> {
        let mut conditions = Vec::new();
        let mut trivially_true = false;
        for selector in selectors {
            let seqs = collect_seqs(selector);
            match self.argument_condition(&seqs, context, of_depth)? {
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

    /// The condition imposed on the candidate element by the whole
    /// argument chain. The compound's element becomes a `self::` node
    /// test, which tests exactly what the name would have tested as the
    /// node test of a top-level selector: `:is(p)` constrains the same
    /// elements as `p`, and a prefix still resolves through the caller's
    /// namespace map. A complex argument applies its rightmost
    /// compound to the candidate, with everything to its left becoming an
    /// existence test through reversed axes:
    /// `:is(a > b ~ c)` matches a `c` with a preceding sibling `b` whose
    /// parent is an `a`.
    ///
    /// The chain is walked twice rather than recursed over, so its length
    /// costs no stack: once left-to-right to translate each compound and
    /// pick its reversed axis, then once right-to-left (leftmost compound
    /// first) to wrap each condition inside the one to its right.
    ///
    /// `None` means the chain imposes no condition (a bare `*` argument).
    fn argument_condition(
        &self,
        seqs: &[(Vec<&Component<CssToXpathImpl>>, Option<Combinator>)],
        context: &str,
        of_depth: usize,
    ) -> Result<Option<Condition>, Error> {
        let mut subs: Vec<XPathExpr> = Vec::with_capacity(seqs.len());
        // `axes[i]` points back at where the left-hand side of `seqs[i]`'s
        // combinator must be, relative to the element matched by
        // `seqs[i]`. The leftmost compound has nothing to its left, so
        // there is one fewer axis than compound.
        let mut axes: Vec<&str> = Vec::with_capacity(seqs.len().saturating_sub(1));
        for (idx, (compound, combinator)) in seqs.iter().enumerate() {
            let mut sub = self.compound_to_xpath(compound, of_depth)?;
            sub.take_element_into_self_test();
            subs.push(sub);
            if idx + 1 < seqs.len() {
                axes.push(match combinator {
                    Some(Combinator::Descendant) => "ancestor::*",
                    Some(Combinator::Child) => "parent::*",
                    Some(Combinator::LaterSibling) => "preceding-sibling::*",
                    Some(Combinator::NextSibling) => "preceding-sibling::*[1]",
                    other => {
                        return Err(Error::Unsupported(format!(
                            "an unexpected combinator ({other:?}) inside `{context}`"
                        )));
                    }
                });
            }
        }

        // A single compound imposes its own conditions and nothing else.
        if subs.len() == 1 {
            return Ok(subs.pop().expect("checked").condition());
        }

        // The nesting reads outward-in — `c0 and axis0[c1 and axis1[c2]]`
        // — so write it in that order: each compound emits its own
        // conditions and opens its axis bracket, and every bracket closes
        // at the end. Wrapping the other way (nesting the condition built
        // so far inside the next compound's brackets) would copy the whole
        // accumulated condition once per compound, making a chain of n
        // compounds cost O(n^2) bytes.
        let innermost = subs.last().expect("more than one compound").condition();
        let mut expr = String::new();
        let mut open = 0usize;
        for (idx, (sub, axis)) in subs[..subs.len() - 1].iter().zip(&axes).enumerate() {
            // This compound's own conditions come first, conjoined with
            // the existence test that follows; a lone or-group is
            // parenthesized here because `and` binds tighter than `or`.
            if let Some(condition) = sub.condition() {
                if condition.or_group {
                    expr.push('(');
                    expr.push_str(&condition.expr);
                    expr.push(')');
                } else {
                    expr.push_str(&condition.expr);
                }
                expr.push_str(" and ");
            }
            expr.push_str(axis);
            // The bracket is only opened when something goes inside it:
            // the innermost compound may impose no condition at all (a
            // bare `*`), leaving the axis as a plain existence test.
            if idx + 2 < subs.len() || innermost.is_some() {
                expr.push('[');
                open += 1;
            }
        }
        // The innermost condition sits inside brackets, so a top-level
        // `or` needs no parentheses of its own.
        if let Some(condition) = &innermost {
            expr.push_str(&condition.expr);
        }
        for _ in 0..open {
            expr.push(']');
        }
        Ok(Some(Condition {
            expr,
            // Every compound but the innermost contributes an existence
            // test conjoined at the top level, so the result is an `and`.
            or_group: false,
        }))
    }
}

/// Collect a selector's compound sequences in match order: `seqs[i]` is
/// (compound, combinator between this compound and the one to its left),
/// so `seqs[0]` is the rightmost compound and only the last entry's
/// combinator is `None`.
fn collect_seqs(
    selector: &Selector<CssToXpathImpl>,
) -> Vec<(Vec<&Component<CssToXpathImpl>>, Option<Combinator>)> {
    let mut iter = selector.iter();
    let mut seqs: Vec<(Vec<&Component<CssToXpathImpl>>, Option<Combinator>)> = Vec::new();
    loop {
        let compound: Vec<&Component<CssToXpathImpl>> = (&mut iter).collect();
        let combinator = iter.next_sequence();
        let done = combinator.is_none();
        seqs.push((compound, combinator));
        if done {
            break;
        }
    }
    seqs
}

/// A namespace prefix that is not a valid XPath name cannot appear in a
/// node test, and XPath 1.0 offers no way to resolve it without the
/// namespace URI, which this crate never sees. Comparing the whole
/// `prefix:name` against `name()` instead would match only documents that
/// happen to use that very prefix, so such a prefix errors rather than
/// approximating.
fn unsafe_prefix_error(prefix: &str) -> Error {
    Error::Unsupported(format!(
        "a namespace prefix that needs quoting (`{prefix}`)"
    ))
}

/// Human-readable construct names for unsupported-error messages.
fn describe_component(component: &Component<CssToXpathImpl>) -> String {
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
        // variants require parser features this crate never enables; they
        // are unreachable, but erroring beats panicking (panic = abort
        // would tear down the caller's process).
        other => format!("an unexpected construct ({other:?})"),
    }
}
