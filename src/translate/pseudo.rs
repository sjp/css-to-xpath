//! The non-tree-structural pseudo-class translations: the "never matches"
//! set, the HTML overrides, and `:lang()`/`:dir()`.
//!
//! Both `html` and `xhtml` use the HTML overrides (they differ in the
//! lowercasing flags and in where `:lang()` reads an element's language
//! from); the generic translator answers `0` (never matches) for
//! everything except `:lang()`, which it maps to XPath's `lang()`
//! function.

use crate::parser::PseudoClass;

use super::error::Error;
use super::xpath_expr::{Condition, XPathExpr, ascii_lower, xpath_literal};
use super::{Kind, Translator};

/// Where a translator reads an element's language from, for `:lang()`.
/// The two halves — which elements carry a language, and what that
/// element's language string is — are the only things that differ
/// between the flavours, so every `:lang()` condition is built from
/// [`LangSource::nearest`] plus [`LangSource::string`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LangSource {
    /// Generic: `xml:lang`, which is what XPath's own `lang()` reads.
    /// Only the wildcard range needs it spelled out; every other range
    /// goes through `lang()` itself. XML binds the `xml` prefix
    /// implicitly, so it needs no entry in the caller's namespace map —
    /// libxml2 pre-binds it; a processor that resolves prefixes purely
    /// from a caller-supplied map (sxd-xpath, say) needs it registered.
    XmlLang,
    /// `html`: the `lang` content attribute alone. An HTML parser puts a
    /// literal `xml:lang` in no namespace on HTML elements, and HTML's
    /// language determination ignores it, so `@lang` is the whole story.
    Lang,
    /// `xhtml`: either attribute, since XHTML documents conventionally
    /// carry `xml:lang` (often alongside `lang`). HTML's language
    /// determination takes the nearest ancestor-or-self with either one
    /// and prefers `xml:lang` when both sit on that element.
    Both,
}

impl LangSource {
    /// The nearest ancestor-or-self carrying a language attribute (`[1]`
    /// counts backwards along a reverse axis). An element with the
    /// attribute but an empty value still stops the walk: an empty value
    /// resets the language to unknown rather than deferring to a further
    /// ancestor.
    fn nearest(self) -> &'static str {
        match self {
            LangSource::XmlLang => "ancestor-or-self::*[@xml:lang][1]",
            LangSource::Lang => "ancestor-or-self::*[@lang][1]",
            LangSource::Both => "ancestor-or-self::*[@xml:lang or @lang][1]",
        }
    }

    /// That element's language string, as an expression evaluated with
    /// the element as the context node.
    ///
    /// For [`LangSource::Both`], `xml:lang` wins whenever it is present.
    /// XPath 1.0 has no conditional, so the `lang` half is truncated to
    /// zero length (`string-length(@lang) * not(@xml:lang)` multiplies
    /// the length by 0 when `xml:lang` is there) and `concat` contributes
    /// `""` for a missing attribute — leaving exactly one of the two.
    fn string(self) -> &'static str {
        match self {
            LangSource::XmlLang => "@xml:lang",
            LangSource::Lang => "@lang",
            LangSource::Both => {
                "concat(@xml:lang, \
                 substring(@lang, 1, string-length(@lang) * not(@xml:lang)))"
            }
        }
    }
}

/// The HTML `type` attribute, ASCII-lowercased so comparisons against
/// enumerated-attribute keywords are case-insensitive: `type` is an
/// [enumerated attribute](https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute),
/// so `type="RADIO"` is a radio and `type="HIDDEN"` is hidden. This is the
/// same ASCII fold `[type=...]` itself gets (`Translator::apply_case_flag`),
/// through the same helper.
fn type_lc() -> String {
    ascii_lower("@type")
}

/// Every element name in the HTML overrides is matched by `local-name()`,
/// never by a qualified name or a bare node test. The overrides are the one
/// part of a translation the caller cannot spell themselves, and the
/// document they run against may put HTML's elements in a namespace: in
/// XHTML they are in `http://www.w3.org/1999/xhtml`, so `ancestor::fieldset`
/// matches nothing, and under a bound prefix (`<h:input>`) a qualified-name
/// comparison against `'input'` fails too. Matching by local name makes the
/// fragments work for `*|input` and `h|input` subjects alike, and leaves
/// `Mode::Html` unchanged in meaning — libxml2's HTML parser produces no
/// namespaces, so there `local-name()` and the qualified name always agree.
/// The crate's namespace rule for *written* names (an unprefixed name is
/// the null namespace) is unaffected: that governs the subject the user
/// writes, which is still translated as documented.
///
/// A form control is disabled by a `fieldset[disabled]` ancestor unless it
/// sits inside that fieldset's first `legend` child (HTML's "actually
/// disabled" carve-out keeps a disabled group's caption usable). Each such
/// first-legend ancestor protects against exactly one disabled fieldset
/// (distinct legends have distinct parents), so the control is
/// fieldset-disabled iff it has more disabled-fieldset ancestors than
/// protecting legends — which counts nested disabled fieldsets correctly.
const FIELDSET_DISABLED: &str = "count(ancestor::*[local-name() = 'fieldset'][@disabled]) > \
     count(ancestor::*[local-name() = 'legend']\
     [not(preceding-sibling::*[local-name() = 'legend'])]\
     [parent::*[local-name() = 'fieldset'][@disabled]])";

/// The elements HTML's `:enabled` and `:disabled` apply to.
/// Form-associated custom elements are in the spec's list too, but
/// nothing in static markup identifies one, so they are left out.
/// Hyperlinks are not in the list — `a[href]` matches `:link`, never
/// `:enabled` — and neither are the obsolete `keygen` and `command`.
const DISABLEABLE: [&str; 7] = [
    "button", "input", "select", "textarea", "optgroup", "option", "fieldset",
];

/// [`DISABLEABLE`] as a condition, for a subject whose local name the
/// compound does not pin.
fn disableable() -> String {
    let names: Vec<String> = DISABLEABLE
        .iter()
        .map(|name| format!("local-name() = '{name}'"))
        .collect();
    format!("({})", names.join(" or "))
}

/// HTML's "actually disabled", to be read together with [`DISABLEABLE`]:
/// `:disabled` is that set and this condition, `:enabled` is that set and
/// not this condition, so the two stay exact complements.
///
/// An element is actually disabled if it carries `@disabled`; an `option`
/// is disabled by a disabled parent `optgroup` as well (the spec walks up
/// to the nearest `optgroup`, which in conforming markup is the parent);
/// and a control — or a nested `fieldset`, which is itself a disabled
/// fieldset — is disabled by a disabled `fieldset` ancestor. The fieldset
/// rule is the one that reaches neither `optgroup` nor `option`.
fn actually_disabled() -> String {
    format!(
        "@disabled or \
         (local-name() = 'option' and parent::*[local-name() = 'optgroup'][@disabled]) or \
         (not(local-name() = 'optgroup' or local-name() = 'option') \
          and {FIELDSET_DISABLED})"
    )
}

/// The `input` types on which `required` has no effect, as a
/// `|`-delimited haystack: `contains()` against it tests all seven with
/// one mention of `@type`, where seven `=` comparisons would repeat the
/// whole `translate()` fold (see [`type_lc`]) seven times.
const REQUIRED_INERT_TYPES: &str = "|hidden|range|color|submit|image|reset|button|";

/// Whether `@type` names one of [`REQUIRED_INERT_TYPES`].
///
/// `contains(haystack, concat('|', type, '|'))` on its own would also
/// accept a value spelling out several keywords in a row
/// (`type="hidden|range"`), so the pipe-free guard keeps the test exact:
/// a value containing `|` is none of the keywords. The comparison folds
/// case because `type` is an HTML enumerated attribute.
fn required_is_inert() -> String {
    let type_lc = type_lc();
    format!(
        "contains('{REQUIRED_INERT_TYPES}', concat('|', {type_lc}, '|')) \
         and not(contains({type_lc}, '|'))"
    )
}

/// The elements the `required` attribute applies to, for `:required` and
/// `:optional` (HTML spec): `select`, `textarea`, and `input` except the
/// types on which `required` has no effect — those match neither
/// pseudo-class, whatever attributes they carry. For a subject whose
/// local name the compound does not pin.
fn required_applies() -> String {
    format!(
        "((local-name() = 'input' and not({})) or \
         local-name() = 'select' or \
         local-name() = 'textarea')",
        required_is_inert()
    )
}

impl Translator {
    pub(crate) fn apply_pseudo_class(
        &self,
        xpath: &mut XPathExpr,
        pc: &PseudoClass,
    ) -> Result<(), Error> {
        // The HTML overrides name elements through `local-name()`, so
        // when the compound already pins the subject's local name every
        // disjunct written for another name is decided here rather than
        // by the XPath engine: only the arm that can match is emitted,
        // and a name outside the pseudo-class's element set collapses to
        // `0`. `None` — a wildcard subject — keeps the full expression.
        // The element part of a compound is always translated before its
        // conditions, so the name is already known by the time any
        // pseudo-class is applied.
        let name = xpath.local_name.clone();
        let name = name.as_deref();
        match (self.kind(), pc) {
            (_, PseudoClass::Dir(_)) => {
                // :dir() matches by *resolved* directionality, which needs
                // runtime bidi resolution, so it never matches — in both
                // translators. A nearest-@dir-ancestor walk (like the HTML
                // :lang() translation) was considered and rejected: it
                // gets dir="auto" (first-strong-character detection),
                // bdi/form-control defaults, and HTML's invalid-value-
                // means-inherit rule wrong, all of which occur in real
                // markup.
                xpath.add_condition("0");
            }
            (Kind::Generic, PseudoClass::Lang(args)) => {
                self.lang_generic(xpath, args)?;
            }
            (Kind::Html, PseudoClass::Lang(args)) => {
                self.lang_html(xpath, args)?;
            }
            // HTML overrides
            (Kind::Html, PseudoClass::Checked) => {
                xpath.push_condition(checked_condition(name));
            }
            // :any-link is :link ∪ :visited. A static document has no
            // visited state, so every link counts as unvisited and the
            // two pseudo-classes coincide — :any-link shares :link's
            // translation verbatim. HTML matches both on an `a` or
            // `area` with an `href`; the `link` element carries an
            // `href` but is not one of the elements HTML requires to
            // match :link/:visited, so it is not in the set.
            (Kind::Html, PseudoClass::Link) | (Kind::Html, PseudoClass::AnyLink) => {
                xpath.add_condition(&match name {
                    Some("a" | "area") => "@href".to_owned(),
                    Some(_) => "0".to_owned(),
                    None => "@href and (local-name() = 'a' or local-name() = 'area')".to_owned(),
                });
            }
            (Kind::Html, PseudoClass::Required) => {
                xpath.add_condition(&required_condition(name, "@required"));
            }
            (Kind::Html, PseudoClass::Optional) => {
                xpath.add_condition(&required_condition(name, "not(@required)"));
            }
            // `:disabled` and `:enabled` are one expression read two
            // ways, so they always partition the element set.
            (Kind::Html, PseudoClass::Disabled) => {
                xpath.push_condition(disabled_condition(name, /* want_disabled = */ true));
            }
            (Kind::Html, PseudoClass::Enabled) => {
                xpath.push_condition(disabled_condition(name, /* want_disabled = */ false));
            }
            // Everything else never matches.
            _ => {
                xpath.add_condition("0");
            }
        }
        Ok(())
    }

    /// Generic `:lang()`: XPath's `lang()` does language-range prefix
    /// matching natively, so `en` and `en-*` both become `lang('en')`-style
    /// tests. A bare `*` matches elements whose language is *known*, which
    /// `lang()` cannot express, so it walks the language source instead.
    fn lang_generic(&self, xpath: &mut XPathExpr, ranges: &[String]) -> Result<(), Error> {
        let mut conditions: Vec<String> = Vec::new();
        for value in ranges {
            check_wildcard_position(value)?;
            if value == "*" {
                conditions.push(lang_known_condition(self.lang_source()));
            } else if let Some(prefix) = value.strip_suffix("-*") {
                // The trailing '-' goes with the wildcard: lang('en-')
                // would never match, since libxml2 expects the argument
                // itself to end at a subtag boundary.
                conditions.push(format!("lang({})", xpath_literal(prefix)));
            } else {
                conditions.push(format!("lang({})", xpath_literal(value)));
            }
        }
        add_lang_conditions(xpath, conditions);
        Ok(())
    }

    /// HTML `:lang()`: the language of the nearest ancestor-or-self that
    /// has one (see [`LangSource`]) is tested with an ASCII-lowercased,
    /// dash-terminated prefix match.
    fn lang_html(&self, xpath: &mut XPathExpr, ranges: &[String]) -> Result<(), Error> {
        let mut conditions: Vec<String> = Vec::new();
        for value in ranges {
            check_wildcard_position(value)?;
            if value == "*" {
                conditions.push(lang_known_condition(self.lang_source()));
            } else {
                // A trailing wildcard ("en-*") matches the same prefix as
                // the range without it ("en"): both stop at a subtag
                // boundary.
                let range = value.strip_suffix("-*").unwrap_or(value);
                conditions.push(lang_ancestor_condition(
                    self.lang_source(),
                    &format!("{}-", range.to_ascii_lowercase()),
                ));
            }
        }
        add_lang_conditions(xpath, conditions);
        Ok(())
    }
}

/// `:checked` — a selected `option`, or a checked checkbox/radio `input`.
fn checked_condition(name: Option<&str>) -> Condition {
    let type_lc = type_lc();
    match name {
        Some("option") => plain("@selected"),
        Some("input") => plain(&format!(
            "@checked and ({type_lc} = 'checkbox' or {type_lc} = 'radio')"
        )),
        Some(_) => plain("0"),
        None => or_group(&format!(
            "(@selected and local-name() = 'option') or \
             (@checked and local-name() = 'input' \
             and ({type_lc} = 'checkbox' or {type_lc} = 'radio'))"
        )),
    }
}

/// `:required` and `:optional`, which differ only in `attr` — the test on
/// the `required` attribute itself — and share the element set.
fn required_condition(name: Option<&str>, attr: &str) -> String {
    match name {
        Some("select" | "textarea") => attr.to_owned(),
        Some("input") => format!("{attr} and not({})", required_is_inert()),
        Some(_) => "0".to_owned(),
        None => format!("{attr} and {}", required_applies()),
    }
}

/// `:disabled` (`want_disabled`) and `:enabled`, which are the same
/// element set and the same "actually disabled" condition, negated.
///
/// A pinned local name settles both halves: outside [`DISABLEABLE`]
/// neither pseudo-class matches, and inside it the three arms of
/// [`actually_disabled`] reduce to the one written for that name — the
/// disabled-parent-`optgroup` rule for an `option`, the
/// disabled-`fieldset`-ancestor rule for everything the spec applies it
/// to, and neither for an `optgroup` itself.
fn disabled_condition(name: Option<&str>, want_disabled: bool) -> Condition {
    let Some(name) = name else {
        let (set, actually) = (disableable(), actually_disabled());
        return plain(&if want_disabled {
            format!("{set} and ({actually})")
        } else {
            format!("{set} and not({actually})")
        });
    };
    if !DISABLEABLE.contains(&name) {
        return plain("0");
    }
    let (actually, or_group) = match name {
        "optgroup" => ("@disabled".to_owned(), false),
        "option" => (
            "@disabled or parent::*[local-name() = 'optgroup'][@disabled]".to_owned(),
            true,
        ),
        _ => (format!("@disabled or {FIELDSET_DISABLED}"), true),
    };
    if want_disabled {
        Condition {
            expr: actually,
            or_group,
        }
    } else {
        // `not(...)` supplies its own grouping, whatever is inside it.
        plain(&format!("not({actually})"))
    }
}

/// A condition with no top-level `or`.
fn plain(expr: &str) -> Condition {
    Condition {
        expr: expr.to_owned(),
        or_group: false,
    }
}

/// A condition whose expression has a top-level `or`.
fn or_group(expr: &str) -> Condition {
    Condition {
        expr: expr.to_owned(),
        or_group: true,
    }
}

/// A wildcard is meaningful to the XPath 1.0 translations only as a whole
/// range (`*`) or as the final subtag (`en-*`); RFC 4647 extended filtering
/// also allows it in any interior position (`*-CH`, `de-*-DE`), which
/// neither translator can express, so those ranges are rejected rather than
/// silently over- or under-matching. (The parser has already rejected a
/// `*` that is not a whole subtag, such as `en*`, so a range that passes
/// here is either `*` itself or ends in `-*`.)
fn check_wildcard_position(range: &str) -> Result<(), Error> {
    if let Some(pos) = range.find('*')
        && pos != range.len() - 1
    {
        return Err(Error::unsupported(format!(
            "the :lang() language range {range:?} \
             (a wildcard outside the final subtag)"
        )));
    }
    Ok(())
}

/// The shared condition-combining tail of both `:lang()` translations: a
/// single condition is added as-is, multiple are OR-joined.
fn add_lang_conditions(xpath: &mut XPathExpr, conditions: Vec<String>) {
    match conditions.len() {
        0 => {}
        1 => xpath.add_condition(&conditions[0]),
        _ => xpath.add_or_condition(&conditions.join(" or ")),
    }
}

/// The wildcard range `*`, which matches an element whose language is
/// known. The language comes from the nearest ancestor-or-self carrying a
/// language attribute, and an empty value there resets the language to
/// unknown rather than deferring to a further ancestor — so the nearest
/// one must also be non-empty.
fn lang_known_condition(source: LangSource) -> String {
    format!(
        "{}[string-length({}) > 0]",
        source.nearest(),
        source.string()
    )
}

/// The nearest-ancestor language test: the language string is ASCII-folded
/// and dash-terminated so `en-` prefix-matches `en` and `en-NZ` but not
/// `english`, and `search_prefix` arrives already lowercased and
/// dash-terminated.
fn lang_ancestor_condition(source: LangSource, search_prefix: &str) -> String {
    format!(
        "{}[starts-with(concat(\
         translate({}, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz'), '-'), {})]",
        source.nearest(),
        source.string(),
        xpath_literal(search_prefix)
    )
}
