#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod parser;
mod translate;

pub use parser::MAX_NESTING_DEPTH;
pub use translate::{
    Error, MAX_NTH_OF_BYTES, MAX_NTH_OF_DEPTH, Mode, ParseErrorKind, ParseModeError, Translator,
};

/// A `prefix` that searches the context node and its whole subtree:
/// `"a"` becomes `descendant-or-self::a`.
pub const DESCENDANT_OR_SELF: &str = "descendant-or-self::";

/// A `prefix` that searches the whole document, wherever the expression
/// is evaluated from: `"a"` becomes `//a`.
pub const WHOLE_DOCUMENT: &str = "//";

/// Translate a CSS selector to an XPath 1.0 expression.
///
/// # Arguments
///
/// * `css` — A CSS selector string.
/// * `prefix` — An XPath path prefix prepended verbatim to each
///   selector-group branch, so it must end in something a node test can
///   follow: an axis ([`DESCENDANT_OR_SELF`]) or a step separator
///   ([`WHOLE_DOCUMENT`]). Pass `""` for a bare relative expression.
///   Nothing validates it — `"/html/body "` yields `/html/body div`,
///   which XPath reads as a division, not a path. A selector group
///   anchored on `:scope` ignores `prefix` and anchors on `self::`
///   instead.
/// * `mode` — The translator flavour: [`Mode::Generic`], [`Mode::Html`], or
///   [`Mode::Xhtml`].
///
/// # Errors
///
/// Returns an [`Error`] when the selector is syntactically invalid or uses
/// an unsupported construct.
pub fn css_to_xpath(css: &str, prefix: &str, mode: Mode) -> Result<String, Error> {
    Translator::new(mode).css_to_xpath(css, prefix)
}
