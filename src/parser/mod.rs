//! `SelectorImpl` and `Parser` implementations bridging Servo's `selectors`
//! crate to this crate's translator.

pub mod impls;

use cssparser::{
    Parser as CssParser, ParserInput, SourceLocation, ToCss, Token, match_ignore_ascii_case,
};
use selectors::parser::{
    NonTSPseudoClass, ParseRelative, PseudoElement, SelectorImpl, SelectorList,
    SelectorParseErrorKind,
};
use std::fmt;

pub use impls::CssString;

use crate::translate::error::Error;

#[derive(Clone, Debug)]
pub struct CssToXpathImpl;

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
/// `:enabled`, `:disabled`, and `:lang()`. Any other pseudo name is
/// rejected at parse time (tree-structural pseudos are parsed natively by
/// Servo and never reach this type).
///
/// Policy for what belongs here versus erroring: pseudo-classes whose
/// semantics rest on user or runtime state a static document cannot have
/// (the user-action, link, and target families) parse and never match.
/// Names that are unknown, or whose semantics a static translation could
/// at least partially answer but this crate has not implemented (e.g. the
/// form pseudo-classes `:read-only` or `:placeholder-shown`), error
/// instead, so typos and genuinely missing features stay loud.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PseudoClass {
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
    /// The comma-separated language ranges of `:lang()`, each
    /// reassembled from the tokens it was spelled with (see
    /// [`is_valid_lang_range`]).
    Lang(Vec<String>),
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
pub enum NeverPseudoElement {}

impl ToCss for NeverPseudoElement {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        match *self {}
    }
}

impl PseudoElement for NeverPseudoElement {
    type Impl = CssToXpathImpl;
}

pub struct CssToXpathParser;

impl<'i> selectors::parser::Parser<'i> for CssToXpathParser {
    type Impl = CssToXpathImpl;
    type Error = SelectorParseErrorKind<'i>;

    /// Strict everywhere: a selector that fails to parse must surface an
    /// error, never be silently dropped the way forgiving `:is()`/`:where()`
    /// parsing would.
    fn allow_forgiving_selectors(&self) -> bool {
        false
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

    /// A sentinel "default namespace". Without one, Servo drops the
    /// namespace component from both `e` and `*|e` (they match identically),
    /// but they must translate differently (`e` vs a `local-name()`
    /// test). With it, plain `e` carries `DefaultNamespace("")` — mapped to
    /// "no constraint" — while `*|e` keeps `ExplicitAnyNamespace`. The empty
    /// string can never collide with a real prefix (prefixes are non-empty
    /// idents, and `namespace_for_prefix` is the identity).
    fn default_namespace(&self) -> Option<CssString> {
        Some(CssString::from(""))
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
/// The value is set from the profile that costs the most stack per level:
/// an unoptimized build, which needs roughly 16 KB a level against about
/// 4 KB optimized. 64 levels therefore fits in Rust's default 2 MB
/// spawned-thread stack even in a debug build, with room to spare, and is
/// still far beyond any hand-written selector.
pub const MAX_NESTING_DEPTH: usize = 64;

/// The facts about a selector that must be known before Servo is entered,
/// gathered in one linear walk that skips strings, escapes, and comments.
struct Scan {
    /// Whether the selector uses the Level 4 column combinator `||`.
    /// Outside strings, escapes, and comments a doubled pipe can only be
    /// that combinator (a single `|` occurs in namespace prefixes and
    /// `|=`, never doubled). Servo has no column-combinator support and
    /// its parse error misreads the second pipe as namespace syntax
    /// (`ExplicitNamespaceUnexpectedToken`), so the construct is caught
    /// before parsing and named properly. Column selection has no XPath
    /// 1.0 translation anyway: column membership depends on
    /// `colspan`/`rowspan` layout arithmetic.
    column_combinator: bool,
    /// The deepest parenthesis nesting reached, an upper bound on how far
    /// the parser and translator will recurse.
    max_depth: usize,
}

fn scan(css: &str) -> Scan {
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    let mut depth: usize = 0;
    let mut scan = Scan {
        column_combinator: false,
        max_depth: 0,
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
                b'|' if bytes.get(i + 1) == Some(&b'|') => scan.column_combinator = true,
                b'(' => {
                    depth += 1;
                    scan.max_depth = scan.max_depth.max(depth);
                }
                // Unbalanced closers are Servo's to reject, not this
                // walk's: just never go below zero.
                b')' => depth = depth.saturating_sub(1),
                _ => {}
            },
        }
        i += 1;
    }
    scan
}

/// Parse a full selector list (comma-separated groups).
pub fn parse(css: &str) -> Result<SelectorList<CssToXpathImpl>, Error> {
    let scan = scan(css);
    if scan.column_combinator {
        return Err(Error::Unsupported("the `||` column combinator".into()));
    }
    if scan.max_depth > MAX_NESTING_DEPTH {
        return Err(Error::Unsupported(format!(
            "functional pseudo-classes nested more than {MAX_NESTING_DEPTH} levels deep"
        )));
    }
    let mut input = ParserInput::new(css);
    let mut parser = CssParser::new(&mut input);
    SelectorList::parse(&CssToXpathParser, &mut parser, ParseRelative::No).map_err(|e| {
        let detail = match e.kind {
            cssparser::ParseErrorKind::Basic(ref kind) => format!("{kind:?}"),
            cssparser::ParseErrorKind::Custom(ref kind) => format!("{kind:?}"),
        };
        Error::Parse(detail, e.location.column)
    })
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
