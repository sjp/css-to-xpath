//! Error types for selector translation.
//!
//! Errors always name the selector and the construct. The exact wording
//! here is part of this crate's output contract and is pinned by tests.
//!
//! Both messages are bounded, so that a caller can print one whatever it
//! was handed: the quoted selector is elided past [`MAX_SELECTOR_ECHO`]
//! bytes, and a [`Error::Parse`] caret gutter shows a window of the line
//! the error is on rather than the whole selector. A 30 KB selector
//! therefore still yields a message of a few hundred bytes, with the
//! caret intact.

use unicode_width::UnicodeWidthChar;

/// Bytes of the selector reproduced in a message's opening quote before
/// the rest is elided with `…`. A selector at or under this length is
/// quoted exactly as `{:?}` would quote it.
const MAX_SELECTOR_ECHO: usize = 120;

/// Display columns of the error's line kept around the caret in a
/// [`Error::Parse`] gutter: enough for the offending compound and its
/// neighbours, and narrow enough to survive an 80-column terminal
/// alongside the two-space gutter.
const MAX_GUTTER_WIDTH: usize = 72;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// The selector is not valid CSS (as judged by Servo's parser).
    /// The second field is the 0-indexed *byte* offset of the error
    /// within the selector string, used to render a caret pointer. It is
    /// `selector.len()` for an error at end of input.
    Parse(String, u32),
    /// The selector is valid CSS, but uses a construct outside the
    /// supported set: this crate errors rather than approximating.
    Unsupported(String),
}

impl Error {
    /// Render the user-facing message, naming the offending selector.
    pub fn into_message(self, selector: &str) -> String {
        match self {
            Error::Parse(detail, offset) => {
                let quoted = quote(selector);
                let (line, caret) = gutter(selector, offset as usize);
                format!(
                    "Unable to parse the CSS selector {quoted}: {detail}\n  |\n  | {line}\n  | {caret}"
                )
            }
            Error::Unsupported(construct) => {
                let quoted = quote(selector);
                format!(
                    "The CSS selector {quoted} uses {construct}, which this translator does not support"
                )
            }
        }
    }
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

/// Render a [`Error::Parse`] gutter: the line of `selector` that
/// `offset` falls on, and a caret line whose `^` sits under it.
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
