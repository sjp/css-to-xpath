//! The associated types for `CssToXpathImpl`.
//!
//! A plain reference-counted string is used for every string-ish
//! associated type. This deliberately avoids `string_cache` and its static
//! atom tables — a meaningfully smaller vendored dependency tree.

use std::borrow::Borrow;
use std::fmt;
use std::rc::Rc;

use cssparser::ToCss;
use precomputed_hash::PrecomputedHash;

/// An immutable string, shared rather than copied when Servo clones it:
/// the parser clones the namespace prefix for every compound it
/// qualifies. The empty string is `None`, so building or cloning one never
/// allocates — the no-default-namespace sentinel is one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CssString(Option<Rc<str>>);

impl CssString {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_deref().unwrap_or("")
    }
}

impl<'a> From<&'a str> for CssString {
    fn from(s: &'a str) -> Self {
        CssString((!s.is_empty()).then(|| Rc::from(s)))
    }
}

impl AsRef<str> for CssString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for CssString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl ToCss for CssString {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        // Only exercised by Debug/serialization paths, never by translation.
        cssparser::serialize_identifier(self.as_str(), dest)
    }
}

impl PrecomputedHash for CssString {
    fn precomputed_hash(&self) -> u32 {
        // We never use the selectors crate's matching/bloom-filter machinery,
        // only its parser, so a constant hash is sufficient (and consistent
        // with Eq).
        0
    }
}
