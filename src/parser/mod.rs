//! `SelectorImpl` and `Parser` implementations bridging Servo's `selectors`
//! crate to the selectrs translator.

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
pub struct SelectrsImpl;

impl SelectorImpl for SelectrsImpl {
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

/// One argument to `:lang()`: an ident or string value, or a bare `*`
/// wildcard. These are collected as raw tokens (commas and whitespace are
/// separators); `xx-` followed by `*` is combined into `xx-*` at
/// translation time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LangArg {
    Value(String),
    Star,
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
/// at least partially answer but selectrs has not implemented (e.g. the
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
    Lang(Vec<LangArg>),
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
            PseudoClass::Lang(args) => {
                dest.write_char('(')?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        dest.write_char(' ')?;
                    }
                    match arg {
                        LangArg::Value(v) => cssparser::serialize_identifier(v, dest)?,
                        LangArg::Star => dest.write_char('*')?,
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
    type Impl = SelectrsImpl;

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
    type Impl = SelectrsImpl;
}

pub struct SelectrsParser;

impl<'i> selectors::parser::Parser<'i> for SelectrsParser {
    type Impl = SelectrsImpl;
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

    /// `:lang()` argument grammar: idents, strings, and `*` wildcards,
    /// separated by whitespace and/or commas (commas are pure
    /// separators — leading, trailing, and repeated commas are all
    /// tolerated). At least one argument is required; NUMBER/`+`/`-`
    /// tokens are rejected. `:dir()` is stricter, matching its
    /// selectors-4 grammar: exactly one identifier.
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

        let mut args = Vec::new();
        loop {
            let token = match parser.next() {
                Ok(t) => t.clone(),
                Err(_) => break, // end of the function's arguments
            };
            match token {
                Token::Ident(ref v) => args.push(LangArg::Value(v.as_ref().to_owned())),
                Token::QuotedString(ref v) => args.push(LangArg::Value(v.as_ref().to_owned())),
                Token::Delim('*') => args.push(LangArg::Star),
                Token::Comma => {}
                _ => {
                    return Err(parser.new_custom_error(
                        SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
                    ));
                }
            }
        }
        if args.is_empty() {
            return Err(parser.new_custom_error(
                SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
            ));
        }
        Ok(PseudoClass::Lang(args))
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

/// Whether the selector uses the Level 4 column combinator `||` —
/// outside strings, escapes, and comments, where a doubled pipe can only
/// be that combinator (a single `|` occurs in namespace prefixes and
/// `|=`, never doubled). Servo has no column-combinator support and its
/// parse error misreads the second pipe as namespace syntax
/// (`ExplicitNamespaceUnexpectedToken`), so the construct is caught
/// before parsing and named properly. Column selection has no XPath 1.0
/// translation anyway: column membership depends on `colspan`/`rowspan`
/// layout arithmetic.
fn uses_column_combinator(css: &str) -> bool {
    let bytes = css.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
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
                b'|' if bytes.get(i + 1) == Some(&b'|') => return true,
                _ => {}
            },
        }
        i += 1;
    }
    false
}

/// Parse a full selector list (comma-separated groups).
pub fn parse(css: &str) -> Result<SelectorList<SelectrsImpl>, Error> {
    if uses_column_combinator(css) {
        return Err(Error::Unsupported("the `||` column combinator".into()));
    }
    let mut input = ParserInput::new(css);
    let mut parser = CssParser::new(&mut input);
    SelectorList::parse(&SelectrsParser, &mut parser, ParseRelative::No).map_err(|e| {
        let detail = match e.kind {
            cssparser::ParseErrorKind::Basic(ref kind) => format!("{kind:?}"),
            cssparser::ParseErrorKind::Custom(ref kind) => format!("{kind:?}"),
        };
        Error::Parse(detail, e.location.column)
    })
}
