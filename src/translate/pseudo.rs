//! The non-tree-structural pseudo-class translations: the "never matches"
//! set, the HTML overrides, and `:lang()`/`:dir()`.
//!
//! Both `html` and `xhtml` use the HTML overrides (they differ only in the
//! lowercasing flags); the generic translator answers `0` (never matches)
//! for everything except `:lang()`, which it maps to XPath's `lang()`
//! function.

use crate::parser::{LangArg, PseudoClass};

use super::error::Error;
use super::xpath_expr::{XPathExpr, xpath_literal};
use super::{Kind, Translator};

/// The HTML translators' lang attribute. (The generic translator has no
/// use for one — its `:lang()` goes through XPath's `lang()` function.)
const LANG_ATTRIBUTE: &str = "lang";

/// The HTML `type` attribute, ASCII-lowercased so comparisons against
/// enumerated-attribute keywords are case-insensitive: `type` is an
/// [enumerated attribute](https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute),
/// so `type="RADIO"` is a radio and `type="HIDDEN"` is hidden. This is the
/// same ASCII fold the `i` attribute flag uses (`apply_case_flag`).
const TYPE_LC: &str = "translate(@type, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                       'abcdefghijklmnopqrstuvwxyz')";

/// A form control is disabled by a `fieldset[disabled]` ancestor unless it
/// sits inside that fieldset's first `legend` child (HTML's "actually
/// disabled" carve-out keeps a disabled group's caption usable). Each such
/// first-legend ancestor protects against exactly one disabled fieldset
/// (distinct legends have distinct parents), so the control is
/// fieldset-disabled iff it has more disabled-fieldset ancestors than
/// protecting legends — which counts nested disabled fieldsets correctly.
const FIELDSET_DISABLED: &str = "count(ancestor::fieldset[@disabled]) > \
     count(ancestor::legend[not(preceding-sibling::legend)]\
     [parent::fieldset[@disabled]])";

/// The elements the `required` attribute applies to, for `:required` and
/// `:optional` (HTML spec): `select`, `textarea`, and `input` except the
/// types on which `required` has no effect — those match neither
/// pseudo-class, whatever attributes they carry. The type keywords are
/// matched case-insensitively (see [`TYPE_LC`]).
fn required_applies() -> String {
    format!(
        "((name(.) = 'input' and not(\
         {TYPE_LC} = 'hidden' or \
         {TYPE_LC} = 'range' or \
         {TYPE_LC} = 'color' or \
         {TYPE_LC} = 'submit' or \
         {TYPE_LC} = 'image' or \
         {TYPE_LC} = 'reset' or \
         {TYPE_LC} = 'button')) or \
         name(.) = 'select' or \
         name(.) = 'textarea')"
    )
}

impl Translator {
    pub(crate) fn apply_pseudo_class(
        &self,
        xpath: &mut XPathExpr,
        pc: &PseudoClass,
    ) -> Result<(), Error> {
        match (self.kind, pc) {
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
                xpath.add_or_condition(&format!(
                    "(@selected and name(.) = 'option') or \
                     (@checked \
                     and (name(.) = 'input' or name(.) = 'command')\
                     and ({TYPE_LC} = 'checkbox' or {TYPE_LC} = 'radio'))"
                ));
            }
            // :any-link is :link ∪ :visited. A static document has no
            // visited state, so every link counts as unvisited and the
            // two pseudo-classes coincide — :any-link shares :link's
            // translation verbatim (keeping `link` in the element set
            // for consistency, although HTML's :any-link strictly covers
            // only `a` and `area`).
            (Kind::Html, PseudoClass::Link) | (Kind::Html, PseudoClass::AnyLink) => {
                xpath.add_condition(
                    "@href and (name(.) = 'a' or name(.) = 'link' or name(.) = 'area')",
                );
            }
            (Kind::Html, PseudoClass::Required) => {
                xpath.add_condition(&format!("@required and {}", required_applies()));
            }
            (Kind::Html, PseudoClass::Optional) => {
                xpath.add_condition(&format!("not(@required) and {}", required_applies()));
            }
            (Kind::Html, PseudoClass::Disabled) => {
                xpath.add_or_condition(&format!(
                    "( @disabled and ( \
                     (name(.) = 'input' and not({TYPE_LC} = 'hidden')) or \
                     name(.) = 'button' or \
                     name(.) = 'select' or \
                     name(.) = 'textarea' or \
                     name(.) = 'command' or \
                     name(.) = 'fieldset' or \
                     name(.) = 'optgroup' or \
                     name(.) = 'option' \
                     ) ) or ( ( \
                     (name(.) = 'input' and not({TYPE_LC} = 'hidden')) or \
                     name(.) = 'button' or \
                     name(.) = 'select' or \
                     name(.) = 'textarea' \
                     ) \
                     and {FIELDSET_DISABLED} \
                     )"
                ));
            }
            (Kind::Html, PseudoClass::Enabled) => {
                xpath.add_or_condition(&format!(
                    "(@href and (name(.) = 'a' or name(.) = 'link' or name(.) = 'area')) \
                     or \
                     ((name(.) = 'command' or name(.) = 'fieldset' or name(.) = 'optgroup') \
                     and not(@disabled)) \
                     or \
                     (((name(.) = 'input' and not({TYPE_LC} = 'hidden')) \
                     or name(.) = 'button' \
                     or name(.) = 'select' \
                     or name(.) = 'textarea' \
                     or name(.) = 'keygen') \
                     and not (@disabled or {FIELDSET_DISABLED})) \
                     or (name(.) = 'option' and not(@disabled or \
                     ancestor::optgroup[@disabled]))"
                ));
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
    /// tests and a bare `*` is `true()`.
    fn lang_generic(&self, xpath: &mut XPathExpr, args: &[LangArg]) -> Result<(), Error> {
        let mut conditions: Vec<String> = Vec::new();
        for value in lang_values(args)? {
            if value == "*" {
                conditions.push("true()".to_owned());
            } else if let Some(prefix) = value.strip_suffix('*') {
                // lang('en-') would never match: libxml2 expects the
                // argument itself to end at a subtag boundary.
                let prefix = prefix.trim_end_matches('-');
                conditions.push(format!("lang({})", xpath_literal(prefix)));
            } else {
                conditions.push(format!("lang({})", xpath_literal(&value)));
            }
        }
        add_lang_conditions(xpath, conditions);
        Ok(())
    }

    /// HTML `:lang()`: the nearest `lang`-attributed ancestor-or-self is
    /// tested with a lowercased, dash-terminated prefix match.
    fn lang_html(&self, xpath: &mut XPathExpr, args: &[LangArg]) -> Result<(), Error> {
        let mut conditions: Vec<String> = Vec::new();
        for value in lang_values(args)? {
            if value == "*" {
                // Wildcard * matches any element with a lang attribute.
                conditions.push(format!("ancestor-or-self::*[@{LANG_ATTRIBUTE}]"));
            } else if let Some(prefix) = value.strip_suffix('*') {
                // Wildcard suffix like "en-*": don't add '-' if the prefix
                // already ends with it.
                let search_prefix = if prefix.ends_with('-') {
                    prefix.to_lowercase()
                } else {
                    format!("{}-", prefix.to_lowercase())
                };
                conditions.push(lang_ancestor_condition(&search_prefix));
            } else {
                conditions.push(lang_ancestor_condition(&format!(
                    "{}-",
                    value.to_lowercase()
                )));
            }
        }
        add_lang_conditions(xpath, conditions);
        Ok(())
    }
}

/// Reassemble the raw `:lang()` arguments into language ranges. The
/// tokenizer breaks a wildcard range apart at every `*` — `en-*` arrives as
/// `["en-", *]`, `*-CH` as `[*, "-CH"]`, `de-*-DE` as `["de-", *, "-DE"]` —
/// and drops whitespace, so a range is a maximal run of adjacent pieces.
/// Two facts let us rejoin them without the comma boundaries (which the
/// parser also dropped): a separate range can never start with `-`, so a
/// `Value` beginning with `-` always continues the current range; and a `*`
/// continues the current range only when that range so far ends in `-`
/// (otherwise it is a fresh `*` range). Thus `:lang(*, fr)` stays two
/// ranges while `:lang(*-CH)` rejoins into one.
///
/// A wildcard is meaningful to the XPath 1.0 translations only as a whole
/// range (`*`) or as the final subtag (`en-*`); RFC 4647 extended filtering
/// also allows it in any interior position (`*-CH`, `de-*-DE`), which
/// neither translator can express, so those ranges are rejected rather than
/// silently over- or under-matching.
fn lang_values(args: &[LangArg]) -> Result<Vec<String>, Error> {
    let mut values: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for arg in args {
        match arg {
            // A continuation subtag (always starts with '-') extends the
            // range in progress; with nothing in progress it can only be a
            // (degenerate) range of its own.
            LangArg::Value(v) if v.starts_with('-') && current.is_some() => {
                current.as_mut().unwrap().push_str(v);
            }
            LangArg::Value(v) => {
                if let Some(done) = current.replace(v.clone()) {
                    values.push(done);
                }
            }
            // A '*' glued onto a "<subtag>-" prefix is a trailing wildcard
            // ("en-*"); anywhere else it begins a fresh "*" range.
            LangArg::Star if current.as_deref().is_some_and(|c| c.ends_with('-')) => {
                current.as_mut().unwrap().push('*');
            }
            LangArg::Star => {
                if let Some(done) = current.replace("*".to_owned()) {
                    values.push(done);
                }
            }
        }
    }
    values.extend(current);

    for range in &values {
        if let Some(pos) = range.find('*')
            && pos != range.len() - 1
        {
            return Err(Error::Unsupported(format!(
                "the :lang() language range {range:?} \
                 (a wildcard outside the final subtag)"
            )));
        }
    }
    Ok(values)
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

/// The HTML nearest-ancestor language test.
fn lang_ancestor_condition(search_prefix: &str) -> String {
    format!(
        "ancestor-or-self::*[@{LANG_ATTRIBUTE}][1][starts-with(concat(\
         translate(@{LANG_ATTRIBUTE}, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz'), '-'), {})]",
        xpath_literal(search_prefix)
    )
}
