//! `SelectorImpl` and `Parser` implementations bridging Servo's `selectors`
//! crate to this crate's translator.

mod impls;

use cssparser::{Parser as CssParser, ToCss, Token, match_ignore_ascii_case};
use selectors::parser::{
    Component, NonTSPseudoClass, ParseRelative, PseudoElement, RelativeSelector, Selector,
    SelectorImpl, SelectorList, SelectorParseErrorKind,
};
use selectors::visitor::{SelectorListKind, SelectorVisitor};
use std::fmt;

pub(crate) use impls::CssString;

use crate::translate::error::{Error, ParseErrorKind};

#[derive(Clone, Debug)]
pub(crate) struct CssToXpathImpl;

impl SelectorImpl for CssToXpathImpl {
    type ExtraMatchingData<'a> = ();
    type AttrValue = CssString;
    type Identifier = CssString;
    type LocalName = CssString;
    type NamespaceUrl = CssString;
    type NamespacePrefix = CssString;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = PseudoClass;
    type PseudoElement = NeverPseudoElement;
}

/// The non-tree-structural pseudo-classes the translators know.
/// Everything here is the "never matches" set under the generic
/// translator; the HTML translator overrides `:checked`, `:link`,
/// `:enabled`, `:disabled`, the form-state family (`:read-only`,
/// `:read-write`, `:default`, `:placeholder-shown`), and `:lang()`. Any
/// other pseudo name is rejected at parse time (tree-structural pseudos
/// are parsed natively by Servo and never reach this type).
///
/// Policy for what belongs here versus erroring: pseudo-classes whose
/// semantics rest on user or runtime state a static document cannot have
/// (the user-action, link, and target families) parse and never match, as
/// does `:dir()`, whose *resolved* directionality needs the bidi
/// algorithm rather than the document tree (see `apply_pseudo_class`).
/// Names that are unknown, or whose semantics rest on machinery outside
/// the document tree that a static translation would have to guess at
/// (`:valid` and the constraint-validation family, `:indeterminate`,
/// whose checkbox state is IDL-only and whose radio-group arm XPath 1.0
/// cannot express, `:defined`), error instead, so typos and genuinely
/// missing features stay loud.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PseudoClass {
    AnyLink,
    Link,
    Visited,
    Hover,
    Active,
    Focus,
    FocusWithin,
    FocusVisible,
    Target,
    TargetWithin,
    LocalLink,
    Enabled,
    Disabled,
    Checked,
    Required,
    Optional,
    ReadOnly,
    ReadWrite,
    Default,
    PlaceholderShown,
    /// The comma-separated language ranges of `:lang()`, each
    /// reassembled from the tokens it was spelled with (see
    /// [`is_valid_lang_range`]).
    Lang(Vec<String>),
    /// The single identifier of `:dir()`, kept only so the selector can
    /// be serialized back: the translation never matches whatever it
    /// says, so `:dir(rtl)` and `:dir(foo)` translate alike. Selectors 4
    /// defines `ltr` and `rtl`; any other identifier is accepted rather
    /// than rejected, since no value can change the output.
    Dir(String),
}

impl PseudoClass {
    fn name(&self) -> &'static str {
        match self {
            PseudoClass::AnyLink => "any-link",
            PseudoClass::Link => "link",
            PseudoClass::Visited => "visited",
            PseudoClass::Hover => "hover",
            PseudoClass::Active => "active",
            PseudoClass::Focus => "focus",
            PseudoClass::FocusWithin => "focus-within",
            PseudoClass::FocusVisible => "focus-visible",
            PseudoClass::Target => "target",
            PseudoClass::TargetWithin => "target-within",
            PseudoClass::LocalLink => "local-link",
            PseudoClass::Enabled => "enabled",
            PseudoClass::Disabled => "disabled",
            PseudoClass::Checked => "checked",
            PseudoClass::Required => "required",
            PseudoClass::Optional => "optional",
            PseudoClass::ReadOnly => "read-only",
            PseudoClass::ReadWrite => "read-write",
            PseudoClass::Default => "default",
            PseudoClass::PlaceholderShown => "placeholder-shown",
            PseudoClass::Lang(_) => "lang",
            PseudoClass::Dir(_) => "dir",
        }
    }
}

impl ToCss for PseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_char(':')?;
        dest.write_str(self.name())?;
        match self {
            PseudoClass::Lang(ranges) => {
                dest.write_char('(')?;
                for (i, range) in ranges.iter().enumerate() {
                    if i > 0 {
                        dest.write_str(", ")?;
                    }
                    // A range is written back as the token sequence it
                    // was parsed from: `*` cannot be part of an
                    // identifier, so the pieces around it are serialized
                    // separately (`en-*` as the ident `en-` then `*`).
                    // The empty range has no such token — an identifier
                    // cannot be empty — and is written as the string it
                    // has to be written as.
                    if range.is_empty() {
                        dest.write_str("\"\"")?;
                    }
                    for (j, piece) in range.split('*').enumerate() {
                        if j > 0 {
                            dest.write_char('*')?;
                        }
                        if !piece.is_empty() {
                            cssparser::serialize_identifier(piece, dest)?;
                        }
                    }
                }
                dest.write_char(')')
            }
            PseudoClass::Dir(value) => {
                dest.write_char('(')?;
                cssparser::serialize_identifier(value, dest)?;
                dest.write_char(')')
            }
            _ => Ok(()),
        }
    }
}

impl NonTSPseudoClass for PseudoClass {
    fn is_active_or_hover(&self) -> bool {
        matches!(self, PseudoClass::Active | PseudoClass::Hover)
    }

    fn is_user_action_state(&self) -> bool {
        matches!(
            self,
            PseudoClass::Active
                | PseudoClass::Hover
                | PseudoClass::Focus
                | PseudoClass::FocusWithin
                | PseudoClass::FocusVisible
        )
    }
}

/// Uninhabited: `parse_pseudo_element` is left at its erroring default, so
/// `::before` etc. fail to parse — pseudo-elements are not supported.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NeverPseudoElement {}

impl ToCss for NeverPseudoElement {
    // The standard way to write a total match on an uninhabited type.
    // `&self` here can never be a live reference, so clippy's warning
    // about dereferencing one describes a call that cannot happen.
    #[allow(clippy::uninhabited_references)]
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        match *self {}
    }
}

impl PseudoElement for NeverPseudoElement {}

pub(crate) struct CssToXpathParser<'a> {
    /// Whether Servo may recover from an invalid `:is()` / `:where()`
    /// argument instead of failing the whole parse. Only the retry in
    /// [`parse`] sets this, and it then rejects every recovery bar the
    /// empty argument list.
    forgiving: bool,
    /// The caller's default namespace prefix, or the empty sentinel
    /// (see [`CssToXpathParser::default_namespace`]). Built once per
    /// parse, since Servo asks for it once per compound.
    default_namespace: &'a CssString,
    /// When this parse is one of the truncated probes [`locate`] makes,
    /// the length of the probe's input. A functional pseudo-class whose
    /// arguments run out right there was cut short by the truncation,
    /// not written wrong, so it reports end of input as Servo's own
    /// constructs do: an error that a longer probe could still undo
    /// must not read as one already made.
    probe_end: Option<usize>,
}

/// Why a functional pseudo-class was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rejected {
    /// Its arguments ended before they said enough: `:dir()`,
    /// `:lang(en,`.
    Incomplete,
    /// Something in it is wrong as written: an unknown name, or an
    /// argument token that cannot be where it is.
    Invalid,
}

impl CssToXpathParser<'_> {
    /// The error for a functional pseudo-class rejected for `why`, with
    /// `parser` left where its arguments were rejected.
    fn functional_error(
        &self,
        parser: &CssParser<'_>,
        why: Rejected,
    ) -> cssparser::ParseError<SelectorParseErrorKind> {
        // Arguments that ran out at the end of a probe's input, rather
        // than at their `)`, were cut short by the probe.
        if why == Rejected::Incomplete && self.probe_end == Some(parser.position().byte_index()) {
            return cssparser::ParseError::from_basic_kind(
                cssparser::BasicParseErrorKind::EndOfInput,
            );
        }
        cssparser::ParseError::custom(SelectorParseErrorKind::UnsupportedPseudoClassOrElement)
    }
}

impl<'i> selectors::parser::Parser<'i> for CssToXpathParser<'_> {
    type Impl = CssToXpathImpl;
    type Error = SelectorParseErrorKind;

    /// Strict unless [`parse`] is retrying: a selector that fails to
    /// parse must surface an error, never be silently dropped the way
    /// forgiving `:is()`/`:where()` parsing would.
    fn allow_forgiving_selectors(&self) -> bool {
        self.forgiving
    }

    /// Enable `:is()` and `:where()`.
    fn parse_is_and_where(&self) -> bool {
        true
    }

    /// `:matches()` is the legacy alias for `:is()`.
    fn is_is_alias(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case("matches")
    }

    /// Enable `:has()`. The translator restricts the arguments to
    /// compound selectors (with an optional leading combinator).
    fn parse_has(&self) -> bool {
        true
    }

    /// `:nth-child(an+b of S)` / `:nth-last-child(an+b of S)`,
    /// CSS Selectors Level 4.
    fn parse_nth_child_of(&self) -> bool {
        true
    }

    /// The supported non-tree-structural pseudo-classes: the "never
    /// matches" set plus the HTML-translator overrides. Anything else
    /// errors (see the policy note on `PseudoClass`).
    fn parse_non_ts_pseudo_class(
        &self,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoClass, cssparser::ParseError<Self::Error>> {
        let pc = match_ignore_ascii_case! { &name,
            "any-link" => PseudoClass::AnyLink,
            "link" => PseudoClass::Link,
            "visited" => PseudoClass::Visited,
            "hover" => PseudoClass::Hover,
            "active" => PseudoClass::Active,
            "focus" => PseudoClass::Focus,
            "focus-within" => PseudoClass::FocusWithin,
            "focus-visible" => PseudoClass::FocusVisible,
            "target" => PseudoClass::Target,
            "target-within" => PseudoClass::TargetWithin,
            "local-link" => PseudoClass::LocalLink,
            "enabled" => PseudoClass::Enabled,
            "disabled" => PseudoClass::Disabled,
            "checked" => PseudoClass::Checked,
            "required" => PseudoClass::Required,
            "optional" => PseudoClass::Optional,
            "read-only" => PseudoClass::ReadOnly,
            "read-write" => PseudoClass::ReadWrite,
            "default" => PseudoClass::Default,
            "placeholder-shown" => PseudoClass::PlaceholderShown,
            _ => {
                return Err(cssparser::ParseError::custom(
                    SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
                ));
            },
        };
        Ok(pc)
    }

    /// `:lang()` argument grammar: a comma-separated list of at least
    /// one language range, each an ident or string optionally glued to
    /// `*` wildcards. Whitespace is allowed only around the commas: it
    /// is a range *terminator*, never a separator, so `:lang(en fr)` is
    /// an error rather than two ranges, and `en *` is not the range
    /// `en-*`. A range is assembled here, while the tokens' adjacency is
    /// still known — the tokenizer splits `en-*` into an ident and a
    /// delimiter. NUMBER/`+`/`-` tokens are rejected. `:dir()` is
    /// stricter, matching its selectors-4 grammar: exactly one
    /// identifier.
    ///
    /// What the assembled text *says* is not judged here: a range whose
    /// subtags are malformed (`en-`, `*en`) is a well-formed argument
    /// that no language tag can match, and it is the translators that
    /// reject it — by then the range is in hand and the message can name
    /// it (see `check_lang_range`).
    ///
    /// The non-standard text-content pseudo `:contains()` is deliberately
    /// unsupported and falls through to the rejection arm, as does any
    /// unknown functional pseudo.
    fn parse_non_ts_functional_pseudo_class(
        &self,
        name: cssparser::CowRcStr<'i>,
        parser: &mut CssParser<'i>,
        _after_part: bool,
    ) -> Result<PseudoClass, cssparser::ParseError<Self::Error>> {
        if name.eq_ignore_ascii_case("dir") {
            let value = match parser.next() {
                Ok(Token::Ident(v)) => v.as_ref().to_owned(),
                Ok(_) => return Err(self.functional_error(parser, Rejected::Invalid)),
                Err(_) => return Err(self.functional_error(parser, Rejected::Incomplete)),
            };
            if parser.next().is_ok() {
                return Err(self.functional_error(parser, Rejected::Invalid));
            }
            return Ok(PseudoClass::Dir(value));
        }
        if !name.eq_ignore_ascii_case("lang") {
            return Err(self.functional_error(parser, Rejected::Invalid));
        }

        parse_lang_ranges(parser)
            .map(PseudoClass::Lang)
            .map_err(|why| self.functional_error(parser, why))
    }

    /// Identity mapping: `svg|g` translates to `svg:g` — a prefix-only
    /// namespace model with no URL maps.
    fn namespace_for_prefix(&self, prefix: &CssString) -> Option<CssString> {
        Some(prefix.clone())
    }

    /// The caller's default namespace prefix, or a sentinel standing in
    /// for "none set".
    ///
    /// A default namespace is always reported, because without one Servo
    /// drops the namespace component from both `e` and `*|e` (they match
    /// identically), and the two must translate differently (`e` vs a
    /// `local-name()` test). So with none set, plain `e` carries
    /// `DefaultNamespace("")` — mapped to "no constraint" — while `*|e`
    /// keeps `ExplicitAnyNamespace`. The empty string can never collide
    /// with a real prefix (prefixes are non-empty idents, and
    /// `namespace_for_prefix` is the identity), which is also what makes
    /// an empty configured prefix mean "no default namespace".
    ///
    /// With a prefix set, Servo applies CSS Namespaces 3 for us: the
    /// prefix reaches `DefaultNamespace` for type selectors and for the
    /// implicit universal of a type-less compound, but not for the
    /// featureless compounds of an `:is()` / `:where()` / `:not()`
    /// argument, and a written `h|e` naming the same prefix collapses
    /// onto the same component.
    fn default_namespace(&self) -> Option<CssString> {
        Some(self.default_namespace.clone())
    }
}

/// The body of the `:lang()` argument grammar: the comma-separated
/// ranges, or why the arguments do not spell out at least one range.
/// Assembling happens here rather than at translation time because only
/// the token stream records whether two pieces were adjacent, and
/// adjacency is the whole difference between the range `en-*` and the
/// pair `en-`, `*`.
///
/// A range is any assembled text, the empty string included: `:lang("")`
/// is the Level 4 "language not known" range, and the shapes that cannot
/// match anything are the translators' to reject.
fn parse_lang_ranges(parser: &mut CssParser<'_>) -> Result<Vec<String>, Rejected> {
    let mut ranges: Vec<String> = Vec::new();
    let mut current = String::new();
    // Whether `current` has a piece yet, and whether the next piece
    // would be adjacent to it. The two are distinct because an empty
    // string is a piece: `:lang("" *)` has started a range even though
    // `current` is still empty.
    let mut started = false;
    let mut adjacent = true;
    // Whitespace and comments both terminate a range, so neither may be
    // skipped over here. The loop ends with the function's arguments.
    while let Ok(token) = parser.next_including_whitespace_and_comments().cloned() {
        let piece = match token {
            Token::WhiteSpace(_) | Token::Comment(_) => {
                adjacent = false;
                continue;
            }
            Token::Comma => {
                if !started {
                    // an empty range slot: `,en`, `en,,fr`
                    return Err(Rejected::Invalid);
                }
                ranges.push(std::mem::take(&mut current));
                (started, adjacent) = (false, true);
                continue;
            }
            Token::Ident(ref v) | Token::QuotedString(ref v) => v.as_ref().to_owned(),
            Token::Delim('*') => "*".to_owned(),
            _ => return Err(Rejected::Invalid),
        };
        if started && !adjacent {
            // two ranges with no comma between them
            return Err(Rejected::Invalid);
        }
        current.push_str(&piece);
        started = true;
        // Adjacency is a fact about the gap between two tokens, so it is
        // restored the moment a piece is taken: only whitespace or a
        // comment *after* this one can separate it from the next. Leaving
        // it false here would spread a run of whitespace over the rest of
        // the range and reject a multi-token range after a comma, so that
        // `:lang(*-CH, en)` parsed but `:lang(en, *-CH)` did not.
        adjacent = true;
    }
    if !started {
        // no ranges at all, or a trailing comma
        return Err(Rejected::Incomplete);
    }
    ranges.push(current);
    Ok(ranges)
}

/// The maximum functional-pseudo-class nesting depth accepted, measured
/// as parenthesis nesting in the source selector. Both Servo's parser and
/// this crate's translator recurse once per nesting level (as does
/// dropping the resulting selector tree), so an unbounded depth would
/// overflow the stack — a hard abort, not a panic, so the caller cannot
/// catch it.
///
/// The value is set from the profile that costs the most stack per level,
/// against the smallest stack the crate can be run on. An unoptimized
/// build spends about 16 KB a level (against about 4 KB optimized), so 32
/// levels need roughly 600 KB: a comfortable fit in the 1 MiB a library
/// does not get to choose — the default reserve of a Windows main thread,
/// rustc's `wasm32-unknown-unknown` stack, and whatever a thread pool
/// hands its workers. Sizing against Rust's more generous 2 MB default
/// for a spawned thread instead would let a debug build abort on those
/// targets at a depth this limit promises to accept, which is the limit
/// failing at the one job it has.
///
/// 32 is still far beyond any hand-written selector, and the depth counted
/// is every parenthesis pair, including ones that do not recurse at all
/// (`:nth-child(2n+1)`, `:lang(en)`) and ones that spend two per level
/// (`:nth-child(2 of :is(…))`), so real selectors sit further under it
/// than the number suggests.
pub const MAX_NESTING_DEPTH: usize = 32;

/// The facts about a selector that must be known before Servo is entered,
/// gathered in one linear walk that skips strings, escapes, and comments.
struct Scan {
    /// The byte offset of the first `|` of the Level 4 column combinator
    /// `||`, if the selector uses one. Outside strings, escapes, and
    /// comments a doubled pipe can only be that combinator (a single `|`
    /// occurs in namespace prefixes and `|=`, never doubled). Servo has
    /// no column-combinator support and its parse error misreads the
    /// second pipe as namespace syntax
    /// (`ExplicitNamespaceUnexpectedToken`), so the construct is caught
    /// before parsing and named properly. Column selection has no XPath
    /// 1.0 translation anyway: column membership depends on
    /// `colspan`/`rowspan` layout arithmetic.
    column_combinator: Option<usize>,
    /// The byte offset of the first `(` that opened a level deeper than
    /// [`MAX_NESTING_DEPTH`], if any — the point at which the selector
    /// went too deep for the parser and translator to recurse through
    /// safely, and so the point to put a caret under. The *first* such
    /// parenthesis, not the innermost, so the position does not move
    /// with however much deeper the rest of the selector goes.
    too_deep: Option<usize>,
    /// The byte offset of the first `&` nesting selector, if the
    /// selector uses one. Outside strings, escapes, and comments an `&`
    /// can only be that selector: it appears in no other selector
    /// production. This crate parses with nesting disabled — a `&` has
    /// no meaning without the enclosing rule a selector-to-XPath
    /// function never sees — so Servo does not recognise it as the
    /// start of a compound and fails on whatever comes next instead,
    /// reporting `&` as an empty selector or a dangling combinator.
    /// Catching it here names the construct the caller actually wrote.
    nesting_selector: Option<usize>,
    /// The byte offset of the first `:scope` this crate cannot
    /// translate, and which of the two ways it is out of place. Both
    /// are lexical facts — `:scope` is unsupported inside any
    /// functional pseudo-class argument, and at the top level anywhere
    /// but the leftmost compound of its group — so the walk can decide
    /// them from the parenthesis depth and whether a combinator has
    /// been passed, and hand the translator's own check a position it
    /// has no way to recover. See [`ScopeSite`].
    misplaced_scope: Option<(usize, ScopeSite)>,
    /// The byte offset of the first `:host(`, if the selector uses one.
    /// Shadow-DOM host selection has no XPath 1.0 translation at all,
    /// so — like `||` — the mere presence of the construct is the
    /// error, wherever it sits. Only the functional form is looked for:
    /// a bare `:host` is not a pseudo-class this crate's parser accepts,
    /// so it fails to parse and never reaches translation.
    host: Option<usize>,
}

/// Which of the two positions a [`Scan::misplaced_scope`] was found in,
/// since they are reported as different constructs.
#[derive(Clone, Copy, Eq, PartialEq)]
enum ScopeSite {
    /// Inside a functional pseudo-class argument, where an XPath 1.0
    /// predicate cannot name the context node at all.
    Functional,
    /// At the top level, but not in the leftmost compound of its group
    /// — the one place `:scope` translates, by anchoring the whole
    /// expression on the `self::` axis instead of the caller's prefix.
    NotLeftmost,
}

impl ScopeSite {
    /// The construct phrase for this site, as the object of "uses …".
    /// Worded exactly as the translator's own check words it, so the
    /// two cannot diverge: only the position is new.
    fn construct(self) -> &'static str {
        match self {
            ScopeSite::Functional => "the `:scope` pseudo-class inside a functional pseudo-class",
            ScopeSite::NotLeftmost => "the `:scope` pseudo-class outside the leftmost compound",
        }
    }
}

/// How far into its selector-list group the walk has got, which is all
/// that is needed to place a top-level `:scope`: it is supported in the
/// leftmost compound of a group and nowhere else, so the question is
/// only whether a combinator has been passed since the last top-level
/// comma.
#[derive(Default)]
struct GroupPosition {
    /// Whether anything that is part of a compound has been seen in
    /// this group yet, so that leading whitespace is not a combinator.
    content_seen: bool,
    /// Whether whitespace has been seen after some content: a
    /// descendant combinator if any content follows it, and nothing at
    /// all if the group ends there.
    space_pending: bool,
    /// Whether a combinator — descendant, `>`, `+` or `~` — has been
    /// passed, i.e. whether the leftmost compound is behind the walk.
    combinator_seen: bool,
}

/// The string handling here diverges from the CSS tokenizer on one point:
/// a newline inside a string ends it there, as a bad-string token, where
/// this walk stays "in string" until the closing quote or the end of the
/// input. So the walk can treat as string content — and skip — text the
/// tokenizer reads as syntax, which for `||` only loses a nicer error
/// message and for parentheses could undercount the depth.
///
/// Neither matters, because reaching that state needs a string that no
/// newline-free closing quote follows, and cssparser turns the newline
/// into a bad-string token that fails the parse. The skipped text is
/// everything after that point, so nothing in it is ever parsed, let
/// alone recursed into. Every selector that does parse is one the walk
/// and the tokenizer agree about.
fn scan(css: &str) -> Scan {
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    let mut depth: usize = 0;
    let mut brackets: usize = 0;
    let mut group = GroupPosition::default();
    let mut scan = Scan {
        column_combinator: None,
        too_deep: None,
        nesting_selector: None,
        misplaced_scope: None,
        host: None,
    };
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1; // skip the escaped character
                } else if b == q {
                    quote = None;
                }
            }
            None => {
                // Where the walk is within its selector-list group,
                // which only the top level has: inside a functional
                // argument (`depth > 0`) a `:scope` is unsupported
                // wherever it sits, and inside `[...]` nothing is a
                // combinator — `[a~=b]`'s tilde least of all.
                if depth == 0 && brackets == 0 {
                    match b {
                        b',' => group = GroupPosition::default(),
                        b'>' | b'+' | b'~' => {
                            group.combinator_seen = true;
                            group.content_seen = true;
                            group.space_pending = false;
                        }
                        b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' => {
                            group.space_pending |= group.content_seen;
                        }
                        // A comment is neither content nor whitespace:
                        // it is removed before the selector grammar
                        // sees it, so `a/**/b` is one compound.
                        b'/' if bytes.get(i + 1) == Some(&b'*') => {}
                        _ => {
                            group.combinator_seen |= group.space_pending;
                            group.space_pending = false;
                            group.content_seen = true;
                        }
                    }
                }
                match b {
                    b'\\' => i += 1, // skip the escaped character
                    b'"' | b'\'' => quote = Some(b),
                    b'/' if bytes.get(i + 1) == Some(&b'*') => {
                        // Skip the comment body and its closing "*/".
                        i += 2;
                        while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                            i += 1;
                        }
                        i += 1;
                    }
                    b'|' if bytes.get(i + 1) == Some(&b'|') => {
                        scan.column_combinator.get_or_insert(i);
                    }
                    b'(' => {
                        depth += 1;
                        if depth > MAX_NESTING_DEPTH {
                            scan.too_deep.get_or_insert(i);
                        }
                    }
                    // Unbalanced closers are Servo's to reject, not this
                    // walk's: just never go below zero.
                    b')' => depth = depth.saturating_sub(1),
                    b'[' => brackets += 1,
                    b']' => brackets = brackets.saturating_sub(1),
                    b'&' => {
                        scan.nesting_selector.get_or_insert(i);
                    }
                    // A literal `:scope` / `:host(` here is the
                    // pseudo-class and nothing else — an escaped colon and
                    // a quoted or commented-out one are all skipped above,
                    // and any other ident starting with those letters
                    // (`:scoped`, `::scope`) is a pseudo-class this crate's
                    // parser rejects. Both findings are only ever consulted
                    // after the parse has succeeded, which is what makes
                    // that last part sound.
                    b':' if bytes[i..].starts_with(b":scope") && brackets == 0 => {
                        let site = if depth > 0 {
                            Some(ScopeSite::Functional)
                        } else if group.combinator_seen {
                            Some(ScopeSite::NotLeftmost)
                        } else {
                            None // the leftmost compound, where it is supported
                        };
                        if let Some(site) = site {
                            scan.misplaced_scope.get_or_insert((i, site));
                        }
                    }
                    b':' if bytes[i..].starts_with(b":host(") => {
                        scan.host.get_or_insert(i);
                    }
                    _ => {}
                }
            }
        }
        i += 1;
    }
    scan
}

/// The error one parse attempt produced, before it is turned into an
/// [`Error`] — the kind is needed to tell an empty selector apart from
/// everything else.
type ParseFailure = cssparser::ParseError<SelectorParseErrorKind>;

/// One way of parsing a selector list: strictly or forgivingly, under
/// the caller's default namespace. A failed attempt is located by
/// repeating it on truncations of the input (see [`locate`]), and a
/// repeat has to be the same attempt to fail the same way, so the two
/// settings travel together.
#[derive(Clone, Copy)]
struct Attempt<'a> {
    forgiving: bool,
    default_namespace: &'a CssString,
}

impl Attempt<'_> {
    /// One parse of the whole selector list.
    fn parse(self, css: &str) -> Result<SelectorList<CssToXpathImpl>, ParseFailure> {
        self.run(css, None)
    }

    /// One parse of a truncation of the selector, for [`locate`].
    fn probe(self, css: &str) -> Result<(), ParseFailure> {
        self.run(css, Some(css.len())).map(drop)
    }

    fn run(
        self,
        css: &str,
        probe_end: Option<usize>,
    ) -> Result<SelectorList<CssToXpathImpl>, ParseFailure> {
        SelectorList::parse(
            &CssToXpathParser {
                forgiving: self.forgiving,
                default_namespace: self.default_namespace,
                probe_end,
            },
            &mut CssParser::new(css),
            ParseRelative::No,
        )
    }
}

/// Parse a full selector list (comma-separated groups).
///
/// Selectors 4 gives `:is()` and `:where()` a *forgiving* argument list,
/// of which this crate wants exactly one part: an empty list is valid and
/// matches nothing. Dropping *invalid* arguments is not wanted — a
/// translation library must not quietly ignore what it was handed — so
/// the strict parse decides, and forgiving parsing is only a retry whose
/// result is accepted when the sole thing it recovered from was an empty
/// argument list.
pub(crate) fn parse(
    css: &str,
    default_namespace: Option<&str>,
) -> Result<SelectorList<CssToXpathImpl>, Error> {
    let scan = scan(css);
    if let Some(offset) = scan.column_combinator {
        return Err(Error::unsupported_at("the `||` column combinator", offset));
    }
    if let Some(offset) = scan.too_deep {
        return Err(Error::unsupported_at(
            format!("functional pseudo-classes nested more than {MAX_NESTING_DEPTH} levels deep"),
            offset,
        ));
    }
    // Reported after the other two, so a selector with more than one
    // problem keeps the message it had before this check existed.
    if let Some(offset) = scan.nesting_selector {
        return Err(Error::unsupported_at("the `&` nesting selector", offset));
    }
    let list = parse_lists(css, default_namespace)?;
    // The remaining findings are constructs Servo parses happily and
    // the translator then rejects, so they are consulted only once the
    // parse has succeeded: a selector that is *also* invalid CSS keeps
    // the parse error it has always reported, and the walk never has to
    // be right about text that never parsed. What the walk adds is the
    // position, which the translator — handed components with no source
    // offsets — cannot recover for itself.
    if let Some((offset, site)) = scan.misplaced_scope {
        return Err(Error::unsupported_at(site.construct(), offset));
    }
    if let Some(offset) = scan.host {
        return Err(Error::unsupported_at("the `:host` pseudo-class", offset));
    }
    Ok(list)
}

/// The strict parse, and the forgiving retry that only an empty `:is()`
/// / `:where()` argument list earns.
fn parse_lists(
    css: &str,
    default_namespace: Option<&str>,
) -> Result<SelectorList<CssToXpathImpl>, Error> {
    let default_namespace = CssString::from(default_namespace.unwrap_or(""));
    let strict = Attempt {
        forgiving: false,
        default_namespace: &default_namespace,
    };
    let forgiving = Attempt {
        forgiving: true,
        ..strict
    };
    let strict_error = match strict.parse(css) {
        Ok(list) => return Ok(list),
        Err(e) => e,
    };
    // The strict parse stops at the first error in source order, and an
    // empty argument list is reported as `EmptySelector`. Any other
    // strict error therefore sits before, or is, something the forgiving
    // parse cannot accept cleanly either, and every outcome of the retry
    // below would report it unchanged — so the retry is skipped.
    if !is_empty_selector(&strict_error) {
        return Err(parse_error(css, strict, &strict_error));
    }
    match forgiving.parse(css) {
        Ok(list) if dropped_nothing(&list) => Ok(list),
        // The forgiving parse recovered from a genuinely invalid
        // argument: the strict error is the one that names it, and
        // points at it.
        Ok(_) => Err(parse_error(css, strict, &strict_error)),
        // Both parses failed. An empty argument list is no longer an
        // error, so a strict `EmptySelector` may well be blaming one,
        // while the forgiving parse — which accepts those — stopped at
        // whatever is actually wrong.
        Err(e) if is_empty_selector(&strict_error) => Err(parse_error(css, forgiving, &e)),
        Err(_) => Err(parse_error(css, strict, &strict_error)),
    }
}

/// `e`, the error `attempt` failed `css` with, as this crate reports it.
fn parse_error(css: &str, attempt: Attempt<'_>, e: &ParseFailure) -> Error {
    let failure = locate(css, attempt, &e.kind);
    let mut kind = ParseErrorKind::from_kind(&e.kind, failure.token.as_ref());
    // A kind that echoes the token points at it; one that echoes nothing
    // points where the parse stopped.
    let offset = match kind {
        ParseErrorKind::InvalidPosition | ParseErrorKind::Other(_) => failure.stopped,
        _ => failure.at,
    };
    // `EmptySelector` is what `selectors` reports whenever a compound
    // ends up with no components, which covers both "there was nothing
    // here" and "none of what was here parsed". Only the first is what
    // the name says, so the second is re-reported by its cause.
    if matches!(kind, ParseErrorKind::EmptySelector)
        && let Some(blocked) = blocking_token(css, offset)
    {
        kind = blocked;
    }
    Error::Parse { kind, offset }
}

/// Whether running out of input is itself enough to fail a parse with
/// `kind`: a construct left open, a group with nothing in it, or a
/// combinator with nothing after it. Every other kind needs a token
/// that should not be where it is.
fn is_truncation_kind(kind: &cssparser::ParseErrorKind<SelectorParseErrorKind>) -> bool {
    matches!(
        kind,
        cssparser::ParseErrorKind::Basic(cssparser::BasicParseErrorKind::EndOfInput)
            | cssparser::ParseErrorKind::Custom(
                SelectorParseErrorKind::EmptySelector | SelectorParseErrorKind::DanglingCombinator
            )
    )
}

/// Where a parse failed, as [`locate`] finds it.
struct Failure<'i> {
    /// The byte offset of the token the parse failed on, or of where it
    /// ran out.
    at: usize,
    /// The byte offset the parse had got to when it failed: past the
    /// token it failed on, unless it only looked at that token.
    stopped: usize,
    /// The token the parse failed on, when it failed on one rather than
    /// by running out.
    token: Option<Token<'i>>,
}

/// One token of the selector, and the byte span it was written at.
struct Spanned<'i> {
    start: usize,
    end: usize,
    token: Token<'i>,
}

/// Where a failed `attempt` at `css` went wrong.
///
/// Neither `cssparser` nor `selectors` reports a position, so it is
/// recovered by repeating the attempt on truncations of the input. Servo
/// reads a selector once, left to right, and fails at the first thing it
/// cannot use, so a truncation that keeps that thing fails the same way
/// — and the shortest such truncation ends on it. A truncation that
/// stops short of it can still fail, but only by running out of input,
/// which Servo reports as one of the [truncation
/// kinds](is_truncation_kind) whatever it was in the middle of.
///
/// So a failure of any other kind is located by a binary search for the
/// shortest truncation, cut at a token boundary, that fails with a kind
/// other than those. A failure of one of those kinds is itself a running
/// out, of the input or of a block or a comma-separated group, and is
/// located among the places that can happen instead (see
/// [`locate_truncation`]).
fn locate<'i>(
    css: &'i str,
    attempt: Attempt<'_>,
    kind: &cssparser::ParseErrorKind<SelectorParseErrorKind>,
) -> Failure<'i> {
    let tokens = tokens(css);
    let ran_out = |at| Failure {
        at,
        stopped: at,
        token: None,
    };
    if is_truncation_kind(kind) {
        return ran_out(locate_truncation(css, attempt, &tokens, kind));
    }
    let Some(last) = tokens.len().checked_sub(1) else {
        return ran_out(css.len());
    };
    // The whole selector is known to fail, so the search is over the
    // truncations before it, which only ever make the answer earlier.
    let mut i = tokens[..last].partition_point(|t| match attempt.probe(&css[..t.end]) {
        Ok(()) => true,
        Err(e) => is_truncation_kind(&e.kind),
    });
    // A string or URL that a newline left bad is a good one if the
    // input ends before the newline, so the truncation that keeps it
    // whole succeeds, and the search lands on what follows it.
    if i > 0 && matches!(tokens[i - 1].token, Token::BadString(_) | Token::BadUrl(_)) {
        i -= 1;
    }
    if let cssparser::ParseErrorKind::Custom(
        SelectorParseErrorKind::UnsupportedPseudoClassOrElement,
    ) = kind
    {
        i = pseudo_name(&tokens, i);
    }
    // A token after whitespace can be one the parser only looked at,
    // to see whether the whitespace was a descendant combinator, and
    // then put back: a failure on that combinator stops before it. Any
    // other combinator is read, and a failure on it stops past it.
    let combinator = matches!(tokens[i].token, Token::Delim('>' | '+' | '~'));
    let after_whitespace = tokens[..i]
        .iter()
        .rev()
        .find(|t| !matches!(t.token, Token::Comment(_)))
        .filter(|t| !combinator && matches!(t.token, Token::WhiteSpace(_)));
    Failure {
        at: tokens[i].start,
        stopped: after_whitespace.map_or(tokens[i].end, |t| t.end),
        token: Some(tokens[i].token.clone()),
    }
}

/// The truncation-kind counterpart of [`locate`].
///
/// A block, a comma-separated group, and the input itself all end the
/// same way as far as the parser inside them is concerned: there is
/// nothing more to read. So the end that failed is found the way
/// [`locate`] finds a token, among the truncations just before each
/// `,`, `)` and `]` and at the end of the input — the first of which to
/// fail is the one the error ran out at.
///
/// That end is the position of an [`EndOfInput`]. An empty compound
/// (`EmptySelector`, `DanglingCombinator`) is the end too when nothing
/// came between the compound's start and it (`a > , b`), but when
/// something did, the compound was not empty but unusable (`a > #1abc`),
/// and the position is that of the token the parser could not use. That
/// token is found by another binary search, for the shortest truncation
/// that fails as an empty compound even with a compound added: before
/// the token, a truncation that fails that way is only waiting for a
/// compound, and a `*` completes it; from the token on, nothing can.
///
/// [`EndOfInput`]: cssparser::BasicParseErrorKind::EndOfInput
fn locate_truncation(
    css: &str,
    attempt: Attempt<'_>,
    tokens: &[Spanned<'_>],
    kind: &cssparser::ParseErrorKind<SelectorParseErrorKind>,
) -> usize {
    let ends: Vec<usize> = tokens
        .iter()
        .filter(|t| {
            matches!(
                t.token,
                Token::Comma | Token::CloseParenthesis | Token::CloseSquareBracket
            )
        })
        .map(|t| t.start)
        .collect();
    let end = ends
        .get(ends.partition_point(|&end| attempt.probe(&css[..end]).is_ok()))
        .copied()
        .unwrap_or(css.len());
    if matches!(kind, cssparser::ParseErrorKind::Basic(_)) {
        return end;
    }
    // Whitespace and comments are skipped rather than used, so they are
    // never the token the parser could not use.
    let candidates: Vec<&Spanned<'_>> = tokens
        .iter()
        .take_while(|t| t.end <= end)
        .filter(|t| !matches!(t.token, Token::WhiteSpace(_) | Token::Comment(_)))
        .collect();
    let unusable = candidates.partition_point(|t| {
        let cut = &css[..t.end];
        let awaits_compound = attempt.probe(cut).is_err_and(|e| {
            matches!(
                e.kind,
                cssparser::ParseErrorKind::Custom(
                    SelectorParseErrorKind::EmptySelector
                        | SelectorParseErrorKind::DanglingCombinator
                )
            )
        });
        // The `*` goes in behind a comment, which the parser skips but
        // which keeps it from running into what precedes it: straight
        // after a `/` it would open a comment of its own.
        !(awaits_compound && attempt.probe(&format!("{cut}/**/*")).is_err())
    });
    candidates.get(unusable).map_or(end, |t| t.start)
}

/// The token naming the pseudo-class or pseudo-element that failed at
/// `tokens[i]`: that token itself when it is a name after a colon
/// (`:frobnicate`, `::before`, `:contains(`), and otherwise the function
/// whose arguments it is in — a functional pseudo-class fails on its
/// arguments, and the message names the function.
fn pseudo_name(tokens: &[Spanned<'_>], i: usize) -> usize {
    // Comments between the colon and the name are skipped by the
    // parser, so they are skipped here too.
    let after_colon = tokens[..i]
        .iter()
        .rev()
        .find(|t| !matches!(t.token, Token::Comment(_)))
        .is_some_and(|t| matches!(t.token, Token::Colon));
    if after_colon && matches!(tokens[i].token, Token::Ident(_) | Token::Function(_)) {
        return i;
    }
    // The blocks open before `tokens[i]`, innermost last, so a closing
    // `tokens[i]` counts as inside the block it closes.
    let mut open = Vec::new();
    for (j, t) in tokens[..i].iter().enumerate() {
        match t.token {
            Token::Function(_)
            | Token::ParenthesisBlock
            | Token::SquareBracketBlock
            | Token::CurlyBracketBlock => open.push(j),
            Token::CloseParenthesis | Token::CloseSquareBracket | Token::CloseCurlyBracket => {
                open.pop();
            }
            _ => {}
        }
    }
    // The argument that failed can sit in a block of its own
    // (`:lang([`), so the name is the innermost *function* around it.
    open.into_iter()
        .rev()
        .find(|&j| matches!(tokens[j].token, Token::Function(_)))
        .unwrap_or(i)
}

/// Every token of `css`, whitespace and comments included, with the
/// byte span it was written at.
///
/// `cssparser`'s `Parser` skips the body of a block whose opening token
/// it just handed back, rather than descending into it, so the walk
/// restarts just past every opener: most of a selector's tokens sit
/// inside `[...]` or a functional argument, and a caret has to be able
/// to point at those.
fn tokens(css: &str) -> Vec<Spanned<'_>> {
    let mut tokens = Vec::new();
    let mut base = 0;
    loop {
        let mut parser = CssParser::new(&css[base..]);
        let restart = loop {
            let start = base + parser.position().byte_index();
            // The token is cloned out because reading the position
            // again needs the parser back.
            let Ok(token) = parser.next_including_whitespace_and_comments().cloned() else {
                return tokens;
            };
            let end = base + parser.position().byte_index();
            let opens_block = matches!(
                token,
                Token::Function(_)
                    | Token::ParenthesisBlock
                    | Token::SquareBracketBlock
                    | Token::CurlyBracketBlock
            );
            tokens.push(Spanned { start, end, token });
            if opens_block {
                break end;
            }
        };
        base = restart;
    }
}

/// The token a group blamed for being empty in fact stopped at, when
/// there was one: `#1abc` names an ID that CSS cannot spell unescaped,
/// not an absent selector, and the same hash one compound later
/// (`p#1abc`) is already reported that way.
///
/// `None` when the group really is empty — end of input, or the comma
/// that closes it — leaving [`ParseErrorKind::EmptySelector`] to mean
/// only what it says.
fn blocking_token(css: &str, offset: usize) -> Option<ParseErrorKind> {
    let mut parser = CssParser::new(css.get(offset..)?);
    match parser.next() {
        Ok(Token::Comma) | Err(_) => None,
        Ok(token) => Some(ParseErrorKind::unexpected_token(token)),
    }
}

fn is_empty_selector(e: &ParseFailure) -> bool {
    matches!(
        e.kind,
        cssparser::ParseErrorKind::Custom(SelectorParseErrorKind::EmptySelector)
    )
}

/// Whether a forgiving parse recovered from nothing but empty `:is()` /
/// `:where()` argument lists.
fn dropped_nothing(list: &SelectorList<CssToXpathImpl>) -> bool {
    list.slice()
        .iter()
        .all(|selector| selector.visit(&mut DroppedArgument))
}

/// Finds an argument the forgiving parse dropped. Every `visit_*` method
/// returns `false` to stop the walk the moment one turns up, so a
/// completed walk means there was none.
struct DroppedArgument;

impl SelectorVisitor for DroppedArgument {
    type Impl = CssToXpathImpl;

    fn visit_simple_selector(&mut self, component: &Component<CssToXpathImpl>) -> bool {
        // The empty argument lists are skipped below, so any invalid
        // component reaching here stands for a dropped argument.
        !matches!(component, Component::Invalid(_))
    }

    fn visit_selector_list(
        &mut self,
        _list_kind: SelectorListKind,
        list: &[Selector<CssToXpathImpl>],
    ) -> bool {
        if is_empty_forgiving_list(list) {
            return true;
        }
        list.iter().all(|nested| nested.visit(self))
    }

    fn visit_relative_selector_list(&mut self, list: &[RelativeSelector<CssToXpathImpl>]) -> bool {
        // `:has()` is never parsed forgivingly, but its arguments can
        // nest `:is()`, and the default implementation does not descend.
        list.iter().all(|relative| relative.selector.visit(self))
    }
}

/// Whether `list` is what an empty `:is()` / `:where()` argument list
/// parses to. Forgiving recovery replaces an argument it could not parse
/// with a single [`Component::Invalid`] holding the source text, so an
/// empty list is one such argument whose text holds no tokens: `:is()`,
/// `:is( )`, `:is(/**/)`. A list of two — `:is(a,)` — is a dropped
/// argument, not an empty list.
pub(crate) fn is_empty_forgiving_list(list: &[Selector<CssToXpathImpl>]) -> bool {
    let [selector] = list else {
        return false;
    };
    let mut components = selector.iter_raw_match_order();
    let Some(Component::Invalid(source)) = components.next() else {
        return false;
    };
    if components.next().is_some() {
        return false;
    }
    // Servo keeps the source text it could not parse, so whether the
    // list was empty is decided on that text: nothing but whitespace and
    // comments.
    CssParser::new(source.as_str()).is_exhausted()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(pc: &PseudoClass) -> String {
        let mut s = String::new();
        pc.to_css(&mut s).unwrap();
        s
    }

    #[test]
    fn pseudo_class_to_css_names() {
        assert_eq!(css(&PseudoClass::AnyLink), ":any-link");
        assert_eq!(css(&PseudoClass::Link), ":link");
        assert_eq!(css(&PseudoClass::Visited), ":visited");
        assert_eq!(css(&PseudoClass::Hover), ":hover");
        assert_eq!(css(&PseudoClass::Active), ":active");
        assert_eq!(css(&PseudoClass::Focus), ":focus");
        assert_eq!(css(&PseudoClass::FocusWithin), ":focus-within");
        assert_eq!(css(&PseudoClass::FocusVisible), ":focus-visible");
        assert_eq!(css(&PseudoClass::Target), ":target");
        assert_eq!(css(&PseudoClass::TargetWithin), ":target-within");
        assert_eq!(css(&PseudoClass::LocalLink), ":local-link");
        assert_eq!(css(&PseudoClass::Enabled), ":enabled");
        assert_eq!(css(&PseudoClass::Disabled), ":disabled");
        assert_eq!(css(&PseudoClass::Checked), ":checked");
        assert_eq!(css(&PseudoClass::Required), ":required");
        assert_eq!(css(&PseudoClass::Optional), ":optional");
    }

    #[test]
    fn pseudo_class_to_css_lang() {
        assert_eq!(css(&PseudoClass::Lang(vec!["en".into()])), ":lang(en)");
        assert_eq!(
            css(&PseudoClass::Lang(vec!["en".into(), "fr".into()])),
            ":lang(en, fr)"
        );
        // A wildcard is not part of an identifier, so a range carrying
        // one is written as the tokens it was parsed from.
        assert_eq!(css(&PseudoClass::Lang(vec!["de-*".into()])), ":lang(de-*)");
        assert_eq!(css(&PseudoClass::Lang(vec!["*".into()])), ":lang(*)");
        assert_eq!(
            css(&PseudoClass::Lang(vec!["de-*".into(), "*".into()])),
            ":lang(de-*, *)"
        );
        // Values are run through `serialize_identifier`, not written raw:
        // a leading digit needs escaping to remain a valid CSS identifier.
        assert_eq!(css(&PseudoClass::Lang(vec!["1x".into()])), ":lang(\\31 x)");
        // The empty range has no identifier to write — an identifier
        // cannot be empty — so it is written as the string it was
        // parsed from, and stays a range on the way back in.
        assert_eq!(css(&PseudoClass::Lang(vec![String::new()])), ":lang(\"\")");
        assert_eq!(
            css(&PseudoClass::Lang(vec![String::new(), "en".into()])),
            ":lang(\"\", en)"
        );
    }

    /// The `:lang()` argument grammar, at the level the parser decides
    /// it: whether a token run assembles into ranges at all.
    #[test]
    fn lang_range_grammar() {
        fn ranges(css: &str) -> Option<Vec<String>> {
            let mut parser = CssParser::new(css);
            parser.expect_function_matching("lang").ok()?;
            parser
                .parse_nested_block(|p| {
                    Ok::<_, cssparser::ParseError<()>>(parse_lang_ranges(p).ok())
                })
                .ok()?
        }
        let one = |css: &str, range: &str| {
            assert_eq!(
                ranges(css).as_deref(),
                Some(&[range.to_owned()][..]),
                "{css}"
            );
        };
        one("lang(en)", "en");
        one("lang( en )", "en");
        one("lang(\"en\")", "en");
        one("lang(en-*)", "en-*");
        one("lang(*)", "*");
        one("lang(*-CH)", "*-CH");
        one("lang(\"en nz\")", "en nz");
        // The empty string is a range like any other here: what a range
        // *means* — the empty one included — is settled by the
        // translators, which is also where a malformed one is rejected.
        one("lang(\"\")", "");
        one("lang(en-)", "en-");
        one("lang(--x)", "--x");
        one("lang(en--)", "en--");
        one("lang(en*)", "en*");
        one("lang(*en)", "*en");
        assert_eq!(
            ranges("lang( en , fr )"),
            Some(vec!["en".to_owned(), "fr".to_owned()])
        );
        for css in [
            "lang()",
            "lang(en fr)", // whitespace is not a separator
            "lang(en *)",  // ... and does not build `en-*` either
            "lang(,)",
            "lang(,en)",
            "lang(en,)",
            "lang(en,,fr)",
            "lang(5)",
            "lang(-)",
            "lang(en/**/fr)", // a comment separates tokens as whitespace does
        ] {
            assert_eq!(ranges(css), None, "{css}");
        }
    }

    #[test]
    fn pseudo_class_to_css_dir() {
        assert_eq!(css(&PseudoClass::Dir("ltr".into())), ":dir(ltr)");
    }

    #[test]
    fn pseudo_class_is_active_or_hover() {
        assert!(PseudoClass::Active.is_active_or_hover());
        assert!(PseudoClass::Hover.is_active_or_hover());
        assert!(!PseudoClass::Focus.is_active_or_hover());
        assert!(!PseudoClass::Link.is_active_or_hover());
        assert!(!PseudoClass::Target.is_active_or_hover());
    }

    #[test]
    fn pseudo_class_is_user_action_state() {
        assert!(PseudoClass::Active.is_user_action_state());
        assert!(PseudoClass::Hover.is_user_action_state());
        assert!(PseudoClass::Focus.is_user_action_state());
        assert!(PseudoClass::FocusWithin.is_user_action_state());
        assert!(PseudoClass::FocusVisible.is_user_action_state());
        assert!(!PseudoClass::Link.is_user_action_state());
        assert!(!PseudoClass::Target.is_user_action_state());
        assert!(!PseudoClass::Enabled.is_user_action_state());
        assert!(!PseudoClass::Checked.is_user_action_state());
    }
}

/// Pins the early exit in [`parse_lists`]: it skips the forgiving retry
/// on the grounds that the retry cannot change the result, so the result
/// is checked against the implementation that always made it.
#[cfg(test)]
mod early_exit_tests {
    use super::*;
    use proptest::prelude::*;

    const CORPUS: &str = include_str!("../../tests/corpus/selectors.txt");

    /// `parse_lists` as it was before the early exit: the forgiving
    /// retry after every strict failure.
    fn reference(
        css: &str,
        default_namespace: Option<&str>,
    ) -> Result<SelectorList<CssToXpathImpl>, Error> {
        let default_namespace = CssString::from(default_namespace.unwrap_or(""));
        let strict = Attempt {
            forgiving: false,
            default_namespace: &default_namespace,
        };
        let forgiving = Attempt {
            forgiving: true,
            ..strict
        };
        let strict_error = match strict.parse(css) {
            Ok(list) => return Ok(list),
            Err(e) => e,
        };
        match forgiving.parse(css) {
            Ok(list) if dropped_nothing(&list) => Ok(list),
            Ok(_) => Err(parse_error(css, strict, &strict_error)),
            Err(e) if is_empty_selector(&strict_error) => Err(parse_error(css, forgiving, &e)),
            Err(_) => Err(parse_error(css, strict, &strict_error)),
        }
    }

    fn assert_agrees(css: &str) -> Result<(), TestCaseError> {
        let shown =
            |r: Result<SelectorList<CssToXpathImpl>, Error>| r.map(|list| list.to_css_string());
        prop_assert_eq!(
            shown(parse_lists(css, None)),
            shown(reference(css, None)),
            "{:?}",
            css
        );
        Ok(())
    }

    /// Characters that make up selector syntax, weighted towards the
    /// ones that open, close and separate the constructs at issue.
    const PIECES: &[&str] = &[
        ":is(",
        ":where(",
        ":not(",
        ":has(",
        ":nth-child(",
        "::before",
        ":foo",
        "(",
        ")",
        "[",
        "]",
        ",",
        " ",
        ">",
        "+",
        "~",
        "|",
        "*",
        "=",
        "\"",
        "'",
        "\\",
        "/**/",
        "/*",
        "#",
        ".",
        "a",
        "b",
        "1",
        "-",
        "&",
        "{",
        "}",
        "\u{e9}",
    ];

    fn piece() -> impl Strategy<Value = String> {
        proptest::sample::select(PIECES).prop_map(str::to_owned)
    }

    fn corpus_line() -> impl Strategy<Value = String> {
        let lines: Vec<&'static str> = CORPUS.lines().collect();
        proptest::sample::select(lines).prop_map(str::to_owned)
    }

    /// A corpus selector with a few pieces spliced in at arbitrary
    /// character positions, and a few characters removed.
    fn mutated() -> impl Strategy<Value = String> {
        (
            corpus_line(),
            proptest::collection::vec((any::<proptest::sample::Index>(), piece()), 0..4),
            proptest::collection::vec(any::<proptest::sample::Index>(), 0..3),
        )
            .prop_map(|(line, inserts, removals)| {
                let mut chars: Vec<char> = line.chars().collect();
                for index in removals {
                    if !chars.is_empty() {
                        chars.remove(index.index(chars.len()));
                    }
                }
                for (index, piece) in inserts {
                    let at = index.index(chars.len() + 1);
                    chars.splice(at..at, piece.chars());
                }
                chars.into_iter().collect()
            })
    }

    #[test]
    fn corpus_agrees() {
        for line in CORPUS.lines() {
            assert_agrees(line).unwrap();
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 4096, ..ProptestConfig::default() })]

        #[test]
        fn mutated_corpus_agrees(css in mutated()) {
            assert_agrees(&css)?;
        }

        #[test]
        fn assembled_pieces_agree(pieces in proptest::collection::vec(piece(), 0..12)) {
            assert_agrees(&pieces.concat())?;
        }
    }
}
