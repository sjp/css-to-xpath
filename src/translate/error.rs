//! Error types for selector translation.
//!
//! An [`Error`] is self-describing: its [`Display`](std::fmt::Display)
//! impl names the construct at fault without needing the selector back,
//! so an error that has travelled through a few layers can still be
//! printed. [`Error::message`] additionally takes the selector and
//! renders the full diagnostic — the quoted selector and, whenever the
//! error knows a position, a caret gutter. The exact wording of both is
//! part of this crate's output contract and is pinned by tests.
//!
//! Messages are bounded, so that a caller can print one whatever it was
//! handed: an echoed token is elided past [`MAX_TOKEN_ECHO`] bytes, the
//! quoted selector past [`MAX_SELECTOR_ECHO`] bytes, and a caret gutter
//! shows a window of the line the error is on rather than the whole
//! selector. A 30 KB selector therefore still yields a message of a few
//! hundred bytes, with the caret intact.
//!
//! Nothing here echoes a dependency's `Debug` output: every parse
//! failure is mapped to a [`ParseErrorKind`] of this crate's own, so the
//! text a user sees does not change when `selectors` or `cssparser`
//! renames one of its internal error variants.

use std::fmt;

use cssparser::{BasicParseErrorKind, ParseErrorKind as CssErrorKind, ToCss, Token};
use selectors::parser::SelectorParseErrorKind;
use unicode_width::UnicodeWidthChar;

/// Bytes of the selector reproduced in a message's opening quote before
/// the rest is elided with `…`. A selector at or under this length is
/// quoted exactly as `{:?}` would quote it.
const MAX_SELECTOR_ECHO: usize = 120;

/// Bytes of a single offending token echoed in a [`ParseErrorKind`]
/// before the rest is elided with `…`. Tokens are usually a character or
/// two, but a name is only bounded by the selector's length.
const MAX_TOKEN_ECHO: usize = 40;

/// Display columns of the error's line kept around the caret in a
/// gutter: enough for the offending compound and its neighbours, and
/// narrow enough to survive an 80-column terminal alongside the
/// two-space gutter.
const MAX_GUTTER_WIDTH: usize = 72;

/// Why a CSS selector could not be translated.
///
/// The variants split by *whose* rules were broken: [`Error::Parse`] for
/// a selector CSS itself rejects, [`Error::Unsupported`] for a valid
/// selector this crate declines to approximate.
///
/// A [`Error::Parse`] always knows the offending byte position, so it
/// always renders a caret. A [`Error::Unsupported`] knows one for the
/// constructs the pre-parse scan of the source text finds, and not for
/// the ones the translator finds, because Servo's parsed components
/// carry no source offsets to map a component back to the selector
/// text. Its `offset` is therefore an [`Option`], and its message grows
/// a caret only when it is `Some`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// The selector is not valid CSS (as judged by Servo's parser).
    Parse {
        /// What is wrong with the selector.
        kind: ParseErrorKind,
        /// The 0-indexed *byte* offset of the error within the selector
        /// string, used to render a caret pointer. It is
        /// `selector.len()` for an error at end of input.
        ///
        /// Whenever `kind` echoes a piece of the selector — a token, a
        /// pseudo-class name — this is where that piece was written,
        /// not merely where the parse stopped, so a caller can
        /// highlight it rather than its neighbour. For the kinds that
        /// echo nothing, it is the stopping point.
        offset: usize,
    },
    /// The selector is valid CSS, but uses a construct outside the
    /// supported set: this crate errors rather than approximating.
    Unsupported {
        /// The offending construct, as a noun phrase (`` the `||` column
        /// combinator ``) that reads as the object of "uses …".
        construct: String,
        /// The 0-indexed *byte* offset of the construct within the
        /// selector string, when it is known; `None` when it is not.
        /// See the variant split above for which constructs know it.
        offset: Option<usize>,
    },
}

impl Error {
    /// Render the full user-facing message, naming the offending
    /// selector — and, whenever the error carries a position, pointing a
    /// caret at it.
    ///
    /// [`Display`](std::fmt::Display) is the one-line form for callers
    /// that no longer hold the selector; this is the form to print when
    /// they do.
    #[must_use]
    pub fn message(&self, selector: &str) -> String {
        let quoted = quote(selector);
        match self {
            Error::Parse { kind, offset } => {
                let (line, caret) = gutter(selector, *offset);
                format!(
                    "Unable to parse the CSS selector {quoted}: {kind}\n  |\n  | {line}\n  | {caret}"
                )
            }
            Error::Unsupported {
                construct,
                offset: Some(offset),
            } => {
                let (line, caret) = gutter(selector, *offset);
                format!(
                    "The CSS selector {quoted} uses {construct}, which this translator \
                     does not support\n  |\n  | {line}\n  | {caret}"
                )
            }
            Error::Unsupported {
                construct,
                offset: None,
            } => format!(
                "The CSS selector {quoted} uses {construct}, which this translator does not support"
            ),
        }
    }

    /// Deprecated alias for [`Error::message`], which borrows the error
    /// rather than consuming it.
    #[deprecated(since = "0.3.0", note = "use `Error::message`, which takes `&self`")]
    #[must_use]
    pub fn into_message(self, selector: &str) -> String {
        self.message(selector)
    }

    /// An [`Error::Unsupported`] naming `construct`, with no position:
    /// for the constructs found during translation, which Servo's
    /// offset-free components cannot be mapped back to the source.
    ///
    /// The scan takes over every construct whose supportability is a
    /// *lexical* fact, so that it can carry a position (see `Scan`).
    /// What is left here needs the parsed compound to decide — whether
    /// an of-type pseudo-class has a type to count siblings by, whether
    /// a namespace prefix survives as an XPath name — and locating
    /// *that* would mean a second, approximate model of where the
    /// compounds are, which could point a caret at the wrong one of
    /// several identical-looking constructs. A missing caret is the
    /// better failure, so these stay positionless.
    pub(crate) fn unsupported(construct: impl Into<String>) -> Self {
        Error::Unsupported {
            construct: construct.into(),
            offset: None,
        }
    }

    /// An [`Error::Unsupported`] naming `construct` at `offset`: for the
    /// constructs the pre-parse scan finds, which walks the source text
    /// and so knows where they are.
    pub(crate) fn unsupported_at(construct: impl Into<String>, offset: usize) -> Self {
        Error::Unsupported {
            construct: construct.into(),
            offset: Some(offset),
        }
    }
}

impl fmt::Display for Error {
    /// The one-line form, which does not need the selector: enough to
    /// identify the fault when the error is all a caller still has.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Parse { kind, offset } => {
                write!(f, "invalid CSS selector at byte {offset}: {kind}")
            }
            Error::Unsupported {
                construct,
                offset: Some(offset),
            } => {
                write!(f, "unsupported CSS construct at byte {offset}: {construct}")
            }
            Error::Unsupported {
                construct,
                offset: None,
            } => {
                write!(f, "unsupported CSS construct: {construct}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// What is wrong with a selector that is not valid CSS.
///
/// This is a translation of the `selectors`/`cssparser` error kinds into
/// wording of this crate's own, not a re-export of them: their variants
/// are internal to those crates and several are unreachable from a
/// selector parse. Anything with no closer match — including a variant a
/// future version of either crate adds — becomes [`ParseErrorKind::Other`],
/// so a dependency bump cannot turn into a panic.
///
/// Payloads that echo the selector (a token, a pseudo-class name) are
/// sanitized: control characters are replaced, and the text is elided
/// past 40 bytes (`MAX_TOKEN_ECHO`), so a message stays printable
/// however long the selector is.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// The selector, or one group of a selector list, has nothing in
    /// it: `""`, `"  "`, `"a, , b"`. A group that *had* something, none
    /// of which parsed — `#1abc` — is reported by the token that stopped
    /// it instead.
    EmptySelector,
    /// A combinator with nothing after it, as in `div > `.
    DanglingCombinator,
    /// The selector ends in the middle of a construct.
    EndOfInput,
    /// A construct in a position that does not allow it: a
    /// pseudo-element inside `:is()`, a combinator after one, a `:has()`
    /// nested in another `:has()`.
    InvalidPosition,
    /// A token that cannot appear where it does, as CSS source text.
    UnexpectedToken(String),
    /// A name was required — after `.` or `::` — and something else was
    /// found. Holds the offending token as CSS source text.
    ExpectedName(String),
    /// A pseudo-class or pseudo-element outside the supported set (which
    /// is every pseudo-element: XPath 1.0 has no notion of one). Holds
    /// the name, without its leading colons.
    UnsupportedPseudo(String),
    /// Something that cannot appear inside `[...]`, as CSS source text:
    /// a malformed attribute name, operator, or value.
    InvalidAttributeSelector(String),
    /// A parse failure with no more specific kind here, already worded
    /// as a phrase to be printed after "…selector `x`: ".
    Other(String),
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::EmptySelector => f.write_str("the selector is empty"),
            ParseErrorKind::DanglingCombinator => f.write_str("a combinator with nothing after it"),
            ParseErrorKind::EndOfInput => f.write_str("the selector ends unexpectedly"),
            ParseErrorKind::InvalidPosition => {
                f.write_str("a construct that is not allowed in this position")
            }
            ParseErrorKind::UnexpectedToken(token) => write!(f, "unexpected `{token}`"),
            ParseErrorKind::ExpectedName(token) => write!(f, "expected a name, found `{token}`"),
            ParseErrorKind::UnsupportedPseudo(name) => {
                // Named without its colons: the parser cannot tell how
                // many were written, and `::before` reported as
                // `` `:before` `` would be a third thing again.
                write!(
                    f,
                    "`{name}` is not a supported pseudo-class or pseudo-element"
                )
            }
            ParseErrorKind::InvalidAttributeSelector(token) => {
                write!(f, "`{token}` is not valid in an attribute selector")
            }
            ParseErrorKind::Other(detail) => f.write_str(detail),
        }
    }
}

impl ParseErrorKind {
    /// Translate one parse failure from the dependencies' vocabulary
    /// into this crate's.
    ///
    /// The arms cover every kind the two crates actually produce while
    /// parsing a selector; the rest — the `@`-rule kinds, which need a
    /// stylesheet, and the several `selectors` variants nothing
    /// constructs — fall through to [`ParseErrorKind::Other`], as would a
    /// variant added upstream.
    pub(crate) fn from_kind(kind: &CssErrorKind<'_, SelectorParseErrorKind<'_>>) -> Self {
        use SelectorParseErrorKind as S;
        match kind {
            // A token after an explicit namespace prefix (`ns|5`) is
            // just a token in the wrong place, so it joins the basic
            // kind rather than earning wording of its own.
            CssErrorKind::Basic(BasicParseErrorKind::UnexpectedToken(t))
            | CssErrorKind::Custom(S::ExplicitNamespaceUnexpectedToken(t)) => {
                ParseErrorKind::UnexpectedToken(token_text(t))
            }
            CssErrorKind::Basic(BasicParseErrorKind::EndOfInput) => ParseErrorKind::EndOfInput,
            CssErrorKind::Custom(S::EmptySelector) => ParseErrorKind::EmptySelector,
            CssErrorKind::Custom(S::DanglingCombinator) => ParseErrorKind::DanglingCombinator,
            CssErrorKind::Custom(S::InvalidState) => ParseErrorKind::InvalidPosition,
            CssErrorKind::Custom(S::ClassNeedsIdent(t) | S::PseudoElementExpectedIdent(t)) => {
                ParseErrorKind::ExpectedName(token_text(t))
            }
            CssErrorKind::Custom(S::UnsupportedPseudoClassOrElement(name)) => {
                ParseErrorKind::UnsupportedPseudo(elide(sanitize(name)))
            }
            CssErrorKind::Custom(
                S::NoQualifiedNameInAttributeSelector(t)
                | S::InvalidQualNameInAttr(t)
                | S::ExpectedBarInAttr(t)
                | S::UnexpectedTokenInAttributeSelector(t)
                | S::BadValueInAttr(t),
            ) => ParseErrorKind::InvalidAttributeSelector(token_text(t)),
            CssErrorKind::Custom(S::ExpectedNamespace(prefix)) => ParseErrorKind::Other(format!(
                "the namespace prefix `{}` is not declared",
                elide(sanitize(prefix))
            )),
            _ => ParseErrorKind::Other("the selector is not valid CSS".to_owned()),
        }
    }

    /// A [`ParseErrorKind::UnexpectedToken`] naming `token`, for the
    /// caller that has a token in hand rather than a dependency error
    /// holding one. Sanitized and elided exactly as the mapped kinds
    /// are.
    pub(crate) fn unexpected_token(token: &Token<'_>) -> Self {
        ParseErrorKind::UnexpectedToken(token_text(token))
    }

    /// Whether `token` is the one this kind's message echoes.
    ///
    /// The parser asks this of the tokens on either side of the
    /// position its dependencies reported, to put the caret on the
    /// token the message names rather than next to it. The comparison
    /// is on the payload, so a candidate matches when it spells the
    /// same way the message does — sanitized and elided included, which
    /// keeps the two sides exactly comparable.
    ///
    /// A pseudo-class name is held without its colons and can have been
    /// written as a plain name or as a function, so it is compared
    /// against the token's own name. The kinds that echo nothing —
    /// there is no token for a caret to move to — never match.
    pub(crate) fn names_token(&self, token: &Token<'_>) -> bool {
        match self {
            ParseErrorKind::UnexpectedToken(text)
            | ParseErrorKind::ExpectedName(text)
            | ParseErrorKind::InvalidAttributeSelector(text) => *text == token_text(token),
            ParseErrorKind::UnsupportedPseudo(name) => match token {
                Token::Ident(written) | Token::Function(written) => {
                    *name == elide(sanitize(written))
                }
                _ => false,
            },
            _ => false,
        }
    }
}

/// A token as the CSS source text it was written as, sanitized and
/// elided for printing. `to_css` writes into a `String` infallibly.
fn token_text(token: &Token<'_>) -> String {
    let mut css = String::new();
    let _ = token.to_css(&mut css);
    elide(sanitize(&css))
}

/// `text` with every control character — which a message must never
/// echo raw into a terminal — replaced by U+FFFD, as the caret gutter
/// does for the selector itself.
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

/// `text`, cut to [`MAX_TOKEN_ECHO`] bytes with a `…` if it is longer.
fn elide(mut text: String) -> String {
    if text.len() > MAX_TOKEN_ECHO {
        text.truncate(char_boundary(&text, MAX_TOKEN_ECHO));
        text.push('…');
    }
    text
}
/// Quote `selector` as `{:?}` would, eliding everything past
/// [`MAX_SELECTOR_ECHO`] bytes with `…` so the message stays printable
/// however long the selector is.
fn quote(selector: &str) -> String {
    if selector.len() <= MAX_SELECTOR_ECHO {
        return format!("{selector:?}");
    }
    let head = &selector[..char_boundary(selector, MAX_SELECTOR_ECHO)];
    let mut quoted = format!("{head:?}");
    quoted.pop(); // the closing quote, put back after the ellipsis
    quoted.push('…');
    quoted.push('"');
    quoted
}

/// Render a caret gutter: the line of `selector` that `offset` falls
/// on, and a caret line whose `^` sits under it.
///
/// Only that one line is echoed, so a caret on the second line of a
/// multi-line selector does not point into the first line's text, and it
/// is windowed to [`MAX_GUTTER_WIDTH`] display columns around the caret
/// (with `…` standing for what was cut) so a huge selector does not
/// become a huge message.
///
/// The caret is padded by *display width*, not by character or byte
/// count, so it lands under the offending character in wide (East Asian)
/// text too; tabs, whose width depends on tab stops the caret cannot
/// know, and other control characters are replaced by a single-column
/// stand-in rather than echoed raw.
fn gutter(selector: &str, offset: usize) -> (String, String) {
    let offset = char_boundary(selector, offset.min(selector.len()));
    let (start, end) = line_bounds(selector, offset);

    // Each character of the line as it will be shown, its width, and
    // whether it sits before the caret.
    let cells: Vec<(char, usize, bool)> = selector[start..end]
        .char_indices()
        .map(|(i, c)| {
            let (shown, width) = render(c);
            (shown, width, start + i < offset)
        })
        .collect();
    let caret_col: usize = cells.iter().filter(|c| c.2).map(|c| c.1).sum();
    let total: usize = cells.iter().map(|c| c.1).sum();

    // The window, in display columns: centred on the caret, but never
    // starting so late that the caret falls outside it. `span` counts
    // the column one past the line's end, where an end-of-input caret
    // sits.
    let span = total.max(caret_col + 1);
    let win_start = if span <= MAX_GUTTER_WIDTH {
        0
    } else {
        (caret_col.saturating_sub(MAX_GUTTER_WIDTH / 2)).min(span - MAX_GUTTER_WIDTH)
    };
    let win_end = win_start + MAX_GUTTER_WIDTH;

    // Keep whole characters only: a wide one straddling either edge is
    // dropped, which is what the `…` then stands for.
    let mut shown = String::new();
    let mut shown_start = None;
    let mut col = 0;
    for &(c, width, _) in &cells {
        if col >= win_start && col + width <= win_end {
            shown_start.get_or_insert(col);
            shown.push(c);
        }
        col += width;
    }

    let mut line = String::new();
    if win_start > 0 {
        line.push('…');
    }
    line.push_str(&shown);
    if total > win_end {
        line.push('…');
    }
    let pad =
        caret_col.saturating_sub(shown_start.unwrap_or(win_start)) + usize::from(win_start > 0);
    (line, format!("{}^", " ".repeat(pad)))
}

/// How a character of the selector is shown in the gutter, and the
/// display columns it then occupies.
fn render(c: char) -> (char, usize) {
    match c {
        // A tab renders as anything from one to eight columns depending
        // on the terminal's tab stops; a space is the one substitute
        // whose width the caret can count on.
        '\t' => (' ', 1),
        c if c.is_control() => ('\u{FFFD}', 1),
        // `width()` is `None` only for the controls handled above.
        c => (c, c.width().unwrap_or(1)),
    }
}

/// The byte range of the line of `s` containing `offset`, excluding the
/// line break. CSS preprocessing (css-syntax § 3.3) makes `\r\n`, `\r`,
/// `\n` and `\f` all line breaks, and cssparser's line counter follows
/// it, so all four end a line here.
fn line_bounds(s: &str, offset: usize) -> (usize, usize) {
    let bytes = s.as_bytes();
    let start = bytes[..offset]
        .iter()
        .rposition(|b| matches!(b, b'\n' | b'\r' | b'\x0C'))
        .map_or(0, |i| i + 1);
    let end = bytes[offset..]
        .iter()
        .position(|b| matches!(b, b'\n' | b'\r' | b'\x0C'))
        .map_or(s.len(), |i| offset + i);
    (start, end)
}

/// `offset`, moved back to the character boundary at or before it. The
/// offset in a `Parse` error is derived from a source location rather
/// than taken from an index, so it is not assumed to land cleanly.
fn char_boundary(s: &str, mut offset: usize) -> usize {
    while !s.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
