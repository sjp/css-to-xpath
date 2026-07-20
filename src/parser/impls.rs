//! The associated types for `SelectrsImpl`.
//!
//! A plain `String` newtype is used for every string-ish associated type.
//! This deliberately avoids `string_cache` and its static atom tables — a
//! meaningfully smaller vendored dependency tree.

use std::borrow::Borrow;
use std::fmt;

use cssparser::ToCss;
use precomputed_hash::PrecomputedHash;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CssString(pub String);

impl CssString {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'a> From<&'a str> for CssString {
    fn from(s: &'a str) -> Self {
        CssString(s.to_owned())
    }
}

impl AsRef<str> for CssString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for CssString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl ToCss for CssString {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        // Only exercised by Debug/serialization paths, never by translation.
        cssparser::serialize_identifier(&self.0, dest)
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
