//! The attribute-operator translations (`[attr <op> value]`).

use selectors::attr::AttrSelectorOperator;

use super::error::Error;
use super::xpath_expr::{XPathExpr, xpath_literal};

/// Dispatch over `[attr <op> value]`. Attribute *values* keep their
/// case under every translator. Empty values are fine —
/// `xpath_literal("")` is `''` — though `~=`/`^=`/`$=`/`*=` guard
/// them into never-matching `0` conditions.
///
/// None of the substring operators tests that the attribute exists
/// first: a missing attribute is the empty node-set, which every one
/// of these turns into a false condition on its own. `starts-with`,
/// `substring` and `contains` take its string value, `''`, and every
/// value they are compared against here is non-empty; `=` on a
/// node-set is existential, so an empty one is never equal to
/// anything. Adding `{name} and` would only repeat the attribute
/// expression — which for `[attr=value i]` is a whole `translate()`
/// call.
pub(crate) fn attrib_operator(
    xpath: &mut XPathExpr,
    attrib: &str,
    operator: AttrSelectorOperator,
    value: &str,
) -> Result<(), Error> {
    match operator {
        AttrSelectorOperator::Equal => attrib_equals(xpath, attrib, value),
        AttrSelectorOperator::DashMatch => attrib_dashmatch(xpath, attrib, value),
        AttrSelectorOperator::Includes => attrib_includes(xpath, attrib, value),
        AttrSelectorOperator::Prefix => attrib_prefixmatch(xpath, attrib, value),
        AttrSelectorOperator::Suffix => attrib_suffixmatch(xpath, attrib, value),
        AttrSelectorOperator::Substring => attrib_substringmatch(xpath, attrib, value),
    }
    Ok(())
}

/// `[attr=value]`.
pub(crate) fn attrib_equals(xpath: &mut XPathExpr, name: &str, value: &str) {
    xpath.add_condition(&format!("{name} = {}", xpath_literal(value)));
}

/// `[attr~=value]`, and so `.class`. The value must be non-empty and
/// contain no CSS whitespace (`[ \t\r\n\f]`), otherwise the condition
/// can never match.
///
/// The attribute side splits on the narrower set `normalize-space`
/// knows, which is XML white space: space, tab, CR and LF, but not the
/// form feed. Tokens separated by a U+000C therefore stay joined. That
/// is a deliberate trade, not a limit of XPath 1.0: a `translate()`
/// mapping U+000C to a space, wrapped around the attribute before
/// `normalize-space`, would close the gap. XPath 1.0 string literals
/// have no escape syntax, though, so that fix means a raw control
/// character in the output of the most common construct the crate
/// emits. See the README's Approximations.
pub(crate) fn attrib_includes(xpath: &mut XPathExpr, name: &str, value: &str) {
    let matchable = !value.is_empty()
        && !value
            .chars()
            .any(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}'));
    if matchable {
        xpath.add_condition(&format!(
            "contains(concat(' ', normalize-space({name}), ' '), {})",
            xpath_literal(&format!(" {value} "))
        ));
    } else {
        xpath.add_condition("0");
    }
}

/// `[attr|=value]`. An or-group, so it is parenthesized when it is
/// conjoined with the rest of a compound.
pub(crate) fn attrib_dashmatch(xpath: &mut XPathExpr, name: &str, value: &str) {
    xpath.add_or_condition(&format!(
        "{name} = {} or starts-with({name}, {})",
        xpath_literal(value),
        xpath_literal(&format!("{value}-"))
    ));
}

/// `[attr^=value]`.
pub(crate) fn attrib_prefixmatch(xpath: &mut XPathExpr, name: &str, value: &str) {
    if !value.is_empty() {
        xpath.add_condition(&format!("starts-with({name}, {})", xpath_literal(value)));
    } else {
        xpath.add_condition("0");
    }
}

/// `[attr$=value]`.
/// In XPath there is starts-with but not ends-with, hence the oddness.
/// The offset counts characters, not bytes.
pub(crate) fn attrib_suffixmatch(xpath: &mut XPathExpr, name: &str, value: &str) {
    if !value.is_empty() {
        let offset = value.chars().count() - 1;
        xpath.add_condition(&format!(
            "substring({name}, string-length({name})-{offset}) = {}",
            xpath_literal(value)
        ));
    } else {
        xpath.add_condition("0");
    }
}

/// `[attr*=value]`.
pub(crate) fn attrib_substringmatch(xpath: &mut XPathExpr, name: &str, value: &str) {
    if !value.is_empty() {
        xpath.add_condition(&format!("contains({name}, {})", xpath_literal(value)));
    } else {
        xpath.add_condition("0");
    }
}
