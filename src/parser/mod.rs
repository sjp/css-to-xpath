//! `SelectorImpl` and `Parser` implementations bridging Servo's `selectors`
//! crate to this crate's translator.

mod impls;

use cssparser::{
    Parser as CssParser, ParserInput, SourceLocation, ToCss, Token, match_ignore_ascii_case,
};
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
    type Impl = CssToXpathImpl;

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

impl PseudoElement for NeverPseudoElement {
    type Impl = CssToXpathImpl;
}

pub(crate) struct CssToXpathParser<'a> {
    /// Whether Servo may recover from an invalid `:is()` / `:where()`
    /// argument instead of failing the whole parse. Only the retry in
    /// [`parse`] sets this, and it then rejects every recovery bar the
    /// empty argument list.
    forgiving: bool,
    /// The caller's default namespace prefix, or `None` for the sentinel
    /// (see [`CssToXpathParser::default_namespace`]).
    default_namespace: Option<&'a str>,
}

impl<'i> selectors::parser::Parser<'i> for CssToXpathParser<'_> {
    type Impl = CssToXpathImpl;
    type Error = SelectorParseErrorKind<'i>;

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
        location: SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoClass, cssparser::ParseError<'i, Self::Error>> {
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
                return Err(location.new_custom_error(
                    SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
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
    /// delimiter — and is then checked by [`is_valid_lang_range`].
    /// NUMBER/`+`/`-` tokens are rejected. `:dir()` is stricter,
    /// matching its selectors-4 grammar: exactly one identifier.
    ///
    /// The non-standard text-content pseudo `:contains()` is deliberately
    /// unsupported and falls through to the rejection arm, as does any
    /// unknown functional pseudo.
    fn parse_non_ts_functional_pseudo_class<'t>(
        &self,
        name: cssparser::CowRcStr<'i>,
        parser: &mut CssParser<'i, 't>,
        _after_part: bool,
    ) -> Result<PseudoClass, cssparser::ParseError<'i, Self::Error>> {
        if name.eq_ignore_ascii_case("dir") {
            let value = match parser.next() {
                Ok(Token::Ident(v)) => v.as_ref().to_owned(),
                _ => {
                    return Err(parser.new_custom_error(
                        SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                    ));
                }
            };
            if parser.next().is_ok() {
                return Err(parser.new_custom_error(
                    SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                ));
            }
            return Ok(PseudoClass::Dir(value));
        }
        if !name.eq_ignore_ascii_case("lang") {
            return Err(parser.new_custom_error(
                SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
            ));
        }

        match parse_lang_ranges(parser) {
            Some(ranges) => Ok(PseudoClass::Lang(ranges)),
            None => Err(parser.new_custom_error(
                SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
            )),
        }
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
        Some(CssString::from(self.default_namespace.unwrap_or("")))
    }
}

/// The body of the `:lang()` argument grammar: the comma-separated
/// ranges, or `None` if the arguments do not spell out at least one
/// valid range. Assembling happens here rather than at translation time
/// because only the token stream records whether two pieces were
/// adjacent, and adjacency is the whole difference between the range
/// `en-*` and the pair `en-`, `*`.
fn parse_lang_ranges<'i>(parser: &mut CssParser<'i, '_>) -> Option<Vec<String>> {
    let mut ranges: Vec<String> = Vec::new();
    let mut current = String::new();
    // Whether `current` has a piece yet, and whether the next piece
    // would be adjacent to it. The two are distinct because an empty
    // string is a piece: `:lang("" *)` has started a range even though
    // `current` is still empty.
    let mut started = false;
    let mut adjacent = true;
    loop {
        // Whitespace and comments both terminate a range, so neither may
        // be skipped over here.
        let token = match parser.next_including_whitespace_and_comments() {
            Ok(t) => t.clone(),
            Err(_) => break, // end of the function's arguments
        };
        let piece = match token {
            Token::WhiteSpace(_) | Token::Comment(_) => {
                adjacent = false;
                continue;
            }
            Token::Comma => {
                if !started || !is_valid_lang_range(&current) {
                    return None;
                }
                ranges.push(std::mem::take(&mut current));
                (started, adjacent) = (false, true);
                continue;
            }
            Token::Ident(ref v) | Token::QuotedString(ref v) => v.as_ref().to_owned(),
            Token::Delim('*') => "*".to_owned(),
            _ => return None,
        };
        if started && !adjacent {
            return None; // two ranges with no comma between them
        }
        current.push_str(&piece);
        started = true;
    }
    if !started || !is_valid_lang_range(&current) {
        return None; // no ranges at all, or a trailing comma
    }
    ranges.push(current);
    Some(ranges)
}

/// Whether an assembled `:lang()` argument is a language range: one or
/// more non-empty `-`-separated subtags, each either a whole `*` or free
/// of `*` entirely (RFC 4647 extended-language-range, minus its
/// restrictions on subtag length and character set — which cost nothing
/// but never-matching output, unlike the shapes rejected here).
///
/// The wildcard rule is what makes a typo like `:lang(en*)` an error
/// instead of the two ranges `en` and `*`, the second of which matches
/// every element with a known language. The non-empty-subtag rule
/// rejects `""`, `en-`, and `--x`; a trailing `-` in particular reads as
/// a half-written `en-*`.
///
/// Positional restrictions the translators impose on a *valid* wildcard
/// (only `*` or a final `en-*` survive XPath 1.0) belong to translation,
/// not to this grammar.
fn is_valid_lang_range(range: &str) -> bool {
    !range.is_empty()
        && range
            .split('-')
            .all(|subtag| !subtag.is_empty() && (subtag == "*" || !subtag.contains('*')))
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
    let mut scan = Scan {
        column_combinator: None,
        too_deep: None,
        nesting_selector: None,
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
            None => match b {
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
                b'&' => {
                    scan.nesting_selector.get_or_insert(i);
                }
                _ => {}
            },
        }
        i += 1;
    }
    scan
}

/// The error one parse attempt produced, before it is turned into an
/// [`Error`] — the kind is needed to tell an empty selector apart from
/// everything else.
type ParseFailure<'i> = cssparser::ParseError<'i, SelectorParseErrorKind<'i>>;

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
    let strict = match parse_list(css, false, default_namespace) {
        Ok(list) => return Ok(list),
        Err(e) => e,
    };
    match parse_list(css, true, default_namespace) {
        Ok(list) if dropped_nothing(&list) => Ok(list),
        // The forgiving parse recovered from a genuinely invalid
        // argument: the strict error is the one that names it, and
        // points at it.
        Ok(_) => Err(parse_error(css, &strict)),
        // Both parses failed. An empty argument list is no longer an
        // error, so a strict `EmptySelector` may well be blaming one,
        // while the forgiving parse — which accepts those — stopped at
        // whatever is actually wrong.
        Err(e) if is_empty_selector(&strict) => Err(parse_error(css, &e)),
        Err(_) => Err(parse_error(css, &strict)),
    }
}

/// One parse of the whole selector list.
fn parse_list<'i>(
    css: &'i str,
    forgiving: bool,
    default_namespace: Option<&str>,
) -> Result<SelectorList<CssToXpathImpl>, ParseFailure<'i>> {
    let mut input = ParserInput::new(css);
    let mut parser = CssParser::new(&mut input);
    SelectorList::parse(
        &CssToXpathParser {
            forgiving,
            default_namespace,
        },
        &mut parser,
        ParseRelative::No,
    )
}

fn parse_error(css: &str, e: &ParseFailure<'_>) -> Error {
    Error::Parse {
        kind: ParseErrorKind::from_kind(&e.kind),
        offset: byte_offset(css, e.location),
    }
}

fn is_empty_selector(e: &ParseFailure<'_>) -> bool {
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
    let mut input = ParserInput::new(source.as_str());
    CssParser::new(&mut input).is_exhausted()
}

/// The byte offset within `css` that `location` points at.
///
/// A `SourceLocation` cannot be used as an index: its line is 0-indexed,
/// its column is 1-indexed, and — the part that bites — the column
/// counts UTF-16 code units, so a tab counts as one unit but renders as
/// several columns, a CJK character counts as one but renders as two,
/// and a non-BMP character counts as two but is a single character. A
/// byte offset is what the caret renderer needs to look at the source
/// text itself.
fn byte_offset(css: &str, location: SourceLocation) -> usize {
    let bytes = css.as_bytes();
    // Walk to the start of the error's line. `\r\n`, `\r`, `\n` and `\f`
    // are all line breaks, matching cssparser's own line counter.
    let mut offset = 0;
    let mut line = 0;
    while line < location.line && offset < bytes.len() {
        match bytes[offset] {
            b'\r' => {
                offset += 1;
                if bytes.get(offset) == Some(&b'\n') {
                    offset += 1;
                }
                line += 1;
            }
            b'\n' | b'\x0C' => {
                offset += 1;
                line += 1;
            }
            _ => offset += 1,
        }
    }
    // Then across `column - 1` UTF-16 code units of that line. A column
    // in the middle of a surrogate pair is not reachable from a token
    // boundary, but `saturating_sub` keeps one from running away.
    let mut units = location.column.saturating_sub(1);
    for c in css[offset..].chars() {
        if units == 0 {
            break;
        }
        units = units.saturating_sub(c.len_utf16() as u32);
        offset += c.len_utf8();
    }
    offset
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
    }

    /// The `:lang()` argument grammar, at the level the parser decides
    /// it: whether a token run assembles into ranges at all.
    #[test]
    fn lang_range_grammar() {
        fn ranges(css: &str) -> Option<Vec<String>> {
            let mut input = ParserInput::new(css);
            let mut parser = CssParser::new(&mut input);
            parser.expect_function_matching("lang").ok()?;
            parser
                .parse_nested_block(|p| {
                    Ok::<_, cssparser::ParseError<'_, ()>>(parse_lang_ranges(p))
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
        assert_eq!(
            ranges("lang( en , fr )"),
            Some(vec!["en".to_owned(), "fr".to_owned()])
        );
        for css in [
            "lang()",
            "lang(en fr)", // whitespace is not a separator
            "lang(en *)",  // ... and does not build `en-*` either
            "lang(en*)",   // `*` is only ever a whole subtag
            "lang(*en)",
            "lang(\"\")",
            "lang(en-)",
            "lang(--x)",
            "lang(en--)",
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
