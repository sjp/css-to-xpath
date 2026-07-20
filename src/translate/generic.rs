//! The attribute-operator translations (`[attr <op> value]`).

use selectors::attr::AttrSelectorOperator;

use super::Translator;
use super::error::Error;
use super::xpath_expr::{XPathExpr, xpath_literal};

impl Translator {
    /// Dispatch over `[attr <op> value]`. Attribute *values* keep their
    /// case under every translator. Empty values are fine —
    /// `xpath_literal("")` is `''` — though `~=`/`^=`/`$=`/`*=` guard
    /// them into never-matching `0` conditions.
    pub(crate) fn attrib_operator(
        &self,
        xpath: &mut XPathExpr,
        attrib: &str,
        operator: AttrSelectorOperator,
        value: &str,
    ) -> Result<(), Error> {
        match operator {
            AttrSelectorOperator::Equal => self.attrib_equals(xpath, attrib, value),
            AttrSelectorOperator::DashMatch => self.attrib_dashmatch(xpath, attrib, value),
            AttrSelectorOperator::Includes => self.attrib_includes(xpath, attrib, value),
            AttrSelectorOperator::Prefix => self.attrib_prefixmatch(xpath, attrib, value),
            AttrSelectorOperator::Suffix => self.attrib_suffixmatch(xpath, attrib, value),
            AttrSelectorOperator::Substring => self.attrib_substringmatch(xpath, attrib, value),
        }
        Ok(())
    }

    /// `[attr=value]`.
    pub(crate) fn attrib_equals(&self, xpath: &mut XPathExpr, name: &str, value: &str) {
        xpath.add_condition(&format!("{name} = {}", xpath_literal(value)));
    }

    /// `[attr~=value]`. The value must be non-empty and contain no CSS
    /// whitespace (`[ \t\r\n\f]`), otherwise the condition can never match.
    pub(crate) fn attrib_includes(&self, xpath: &mut XPathExpr, name: &str, value: &str) {
        let matchable = !value.is_empty()
            && !value
                .chars()
                .any(|c| matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{c}'));
        if matchable {
            xpath.add_condition(&format!(
                "{name} and contains(concat(' ', normalize-space({name}), ' '), {})",
                xpath_literal(&format!(" {value} "))
            ));
        } else {
            xpath.add_condition("0");
        }
    }

    /// `[attr|=value]`.
    pub(crate) fn attrib_dashmatch(&self, xpath: &mut XPathExpr, name: &str, value: &str) {
        xpath.add_condition(&format!(
            "{name} and ({name} = {} or starts-with({name}, {}))",
            xpath_literal(value),
            xpath_literal(&format!("{value}-"))
        ));
    }

    /// `[attr^=value]`.
    pub(crate) fn attrib_prefixmatch(&self, xpath: &mut XPathExpr, name: &str, value: &str) {
        if !value.is_empty() {
            xpath.add_condition(&format!(
                "{name} and starts-with({name}, {})",
                xpath_literal(value)
            ));
        } else {
            xpath.add_condition("0");
        }
    }

    /// `[attr$=value]`.
    /// In XPath there is starts-with but not ends-with, hence the oddness.
    /// The offset counts characters, not bytes.
    pub(crate) fn attrib_suffixmatch(&self, xpath: &mut XPathExpr, name: &str, value: &str) {
        if !value.is_empty() {
            let offset = value.chars().count() - 1;
            xpath.add_condition(&format!(
                "{name} and substring({name}, string-length({name})-{offset}) = {}",
                xpath_literal(value)
            ));
        } else {
            xpath.add_condition("0");
        }
    }

    /// `[attr*=value]`.
    pub(crate) fn attrib_substringmatch(&self, xpath: &mut XPathExpr, name: &str, value: &str) {
        if !value.is_empty() {
            xpath.add_condition(&format!(
                "{name} and contains({name}, {})",
                xpath_literal(value)
            ));
        } else {
            xpath.add_condition("0");
        }
    }
}
