//! Error types for selector translation.
//!
//! Errors always name the selector and the construct. The exact wording
//! here is part of this crate's output contract and is pinned by tests.

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// The selector is not valid CSS (as judged by Servo's parser).
    /// The second field is the 1-indexed byte column of the error within the
    /// selector string, used to render a caret pointer.
    Parse(String, u32),
    /// The selector is valid CSS, but uses a construct outside the
    /// supported set: this crate errors rather than approximating.
    Unsupported(String),
    /// The requested translator name is not recognised.
    UnknownTranslator(String),
}

impl Error {
    /// Render the user-facing message, naming the offending selector.
    pub fn into_message(self, selector: &str) -> String {
        match self {
            Error::Parse(detail, column) => {
                let caret_pos = (column as usize).saturating_sub(1).min(selector.len());
                let caret_line = format!("{}{}", " ".repeat(caret_pos), "^");
                format!(
                    "Unable to parse the CSS selector {selector:?}: {detail}\n  |\n  | {selector}\n  | {caret_line}"
                )
            }
            Error::Unsupported(construct) => format!(
                "The CSS selector {selector:?} uses {construct}, which this translator does not support"
            ),
            Error::UnknownTranslator(name) => format!("Unknown translator '{name}'"),
        }
    }
}
