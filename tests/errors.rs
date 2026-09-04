//! Every way a translation can fail, and what the resulting `Error`
//! says: which variant, which payload, and how it renders.

use css_to_xpath::{Mode, Translator};

#[test]
fn unsupported_errors() {
    let t = Translator::new(Mode::Generic);
    // The non-standard [a!=b] and :contains() are not supported.
    assert!(t.css_to_xpath("e[foo!=\"bar\"]", "").is_err());
    assert!(t.css_to_xpath("e:contains(\"foo\")", "").is_err());
    assert!(t.css_to_xpath("e::before", "").is_err());
    assert!(t.css_to_xpath("e:", "").is_err());
    assert!(t.css_to_xpath("", "").is_err());
    // A flag requires an operator and value.
    assert!(t.css_to_xpath("[rel i]", "").is_err());
    assert!(t.css_to_xpath("[rel=stylesheet k]", "").is_err());
    assert!(t.css_to_xpath("[rel=stylesheet i i]", "").is_err());
    // Unknown pseudo-classes error.
    assert!(t.css_to_xpath("e:unknown-pseudo", "").is_err());
    assert!(t.css_to_xpath("e:first-line", "").is_err()); // pseudo-element
    // The Level 4 column combinator and grid-structural pseudos have
    // no XPath 1.0 translation: column membership rests on
    // colspan/rowspan layout arithmetic. `||` is caught before Servo
    // misparses it as namespace syntax...
    assert!(t.css_to_xpath("col || td", "").is_err());
    assert!(t.css_to_xpath("col||td", "").is_err());
    assert!(t.css_to_xpath("e:nth-col(2)", "").is_err());
    assert!(t.css_to_xpath("e:nth-last-col(2n)", "").is_err());
    // ...while pipes in strings, escapes, and comments stay valid.
    assert!(t.css_to_xpath("[foo=\"a||b\"]", "").is_ok());
    assert!(t.css_to_xpath("a\\|\\|b", "").is_ok());
    assert!(t.css_to_xpath("a /* || */ b", "").is_ok());
    // The `&` nesting selector is caught by the same scan, in every
    // position it can appear: alone, either side of a combinator,
    // inside a compound, and inside a functional argument. Without the
    // scan Servo — which parses with nesting disabled — blames
    // whatever it failed on next, which never says the `&` is the
    // construct this translator declines to support.
    assert!(t.css_to_xpath("&", "").is_err());
    assert!(t.css_to_xpath("& a", "").is_err());
    assert!(t.css_to_xpath("a &", "").is_err());
    assert!(t.css_to_xpath("a > &", "").is_err());
    assert!(t.css_to_xpath("a&", "").is_err());
    assert!(t.css_to_xpath("a:is(&)", "").is_err());
    assert!(t.css_to_xpath("a:has(> &)", "").is_err());
    // ...and an `&` in a string, an escape, or a comment stays valid,
    // for the same reason a pipe there does.
    assert!(t.css_to_xpath("[foo=\"a&b\"]", "").is_ok());
    assert!(t.css_to_xpath("a\\&b", "").is_ok());
    assert!(t.css_to_xpath("a /* & */ b", "").is_ok());
    // Pseudo-classes outside the never-match policy (see PseudoClass)
    // error rather than silently matching nothing: constraint
    // validation and `:indeterminate`'s IDL-only state rest on
    // machinery a document does not carry, and erroring keeps typos
    // loud.
    assert!(t.css_to_xpath("e:valid", "").is_err());
    assert!(t.css_to_xpath("e:user-invalid", "").is_err());
    assert!(t.css_to_xpath("e:in-range", "").is_err());
    assert!(t.css_to_xpath("e:indeterminate", "").is_err());
    assert!(t.css_to_xpath("e:defined", "").is_err());
    // The form-state pseudo-classes a static translation *can* answer
    // are in the never-match set instead, so they translate here and
    // carry their HTML meaning under `Mode::Html`/`Mode::Xhtml`.
    assert!(t.css_to_xpath("e:read-only", "").is_ok());
    assert!(t.css_to_xpath("e:placeholder-shown", "").is_ok());
    // :scope is supported in the leftmost compound only, and never
    // inside functional pseudo-class arguments (the context node is
    // unreachable from an XPath 1.0 predicate).
    assert!(t.css_to_xpath("a :scope", "").is_err());
    assert!(t.css_to_xpath("a > :scope", "").is_err());
    assert!(t.css_to_xpath(":scope :scope", "").is_err());
    // The scan that places those decides "leftmost compound" from the
    // source text, so the things that only look like combinators must
    // not fool it: a tilde or a space inside `[...]`, a comment
    // between two halves of one compound, and an escaped space, none
    // of which end a compound — nor a preceding group, which a `,`
    // starts afresh.
    assert!(t.css_to_xpath("[foo~=bar]:scope", "").is_ok());
    assert!(t.css_to_xpath("[foo = bar]:scope", "").is_ok());
    assert!(t.css_to_xpath("a/* > */:scope", "").is_ok());
    assert!(t.css_to_xpath("a\\ b:scope", "").is_ok());
    assert!(t.css_to_xpath("a b, :scope", "").is_ok());
    assert!(t.css_to_xpath("[foo=\" :scope\"]", "").is_ok());
    assert!(t.css_to_xpath("a /* :scope */ b", "").is_ok());
    // Inside a functional pseudo-class, the context node is
    // unreachable from an XPath 1.0 predicate: all four entry points
    // report the same construct, each pointing at its own `:scope`.
    let scope_in_functional = |offset| css_to_xpath::Error::Unsupported {
        construct: "the `:scope` pseudo-class inside a functional pseudo-class".to_owned(),
        offset: Some(offset),
    };
    assert_eq!(
        t.css_to_xpath("e:is(:scope)", "").unwrap_err(),
        scope_in_functional(5)
    );
    assert_eq!(
        t.css_to_xpath("e:not(:scope)", "").unwrap_err(),
        scope_in_functional(6)
    );
    assert_eq!(
        t.css_to_xpath("e:has(:scope)", "").unwrap_err(),
        scope_in_functional(6)
    );
    assert_eq!(
        t.css_to_xpath("e:nth-child(2 of :scope)", "").unwrap_err(),
        scope_in_functional(17)
    );
    // The `:host` pseudo-class is the other construct the scan places
    // for the translator. Only its functional form gets this far: a
    // bare `:host` is not a pseudo-class this crate's parser accepts,
    // so it fails to parse instead.
    assert_eq!(
        t.css_to_xpath("e:is(:host(a))", "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "the `:host` pseudo-class".to_owned(),
            offset: Some(5),
        }
    );
    assert!(t.css_to_xpath(":host", "").is_err());
    // A leading combinator is :has()-only; dangling and doubled
    // combinators are parse errors everywhere.
    assert!(t.css_to_xpath("e:is(> a)", "").is_err());
    assert!(t.css_to_xpath("e:has(> > a)", "").is_err());
    assert!(t.css_to_xpath("e:has(>)", "").is_err());
    assert!(t.css_to_xpath("e:has(a >)", "").is_err());
    // Nested :has() is rejected (selectors-4).
    assert!(t.css_to_xpath("e:has(a:has(b))", "").is_err());
    assert!(t.css_to_xpath("e:has(> a:has(b))", "").is_err());
    // of-type pseudos are not implemented on `*` — including compounds
    // that leave the type implicit (`.foo` is `*.foo`) or carry it
    // only inside a pseudo-class argument. XPath 1.0 cannot compare a
    // sibling's name with the matched element's own name, so only a
    // type named in the compound itself gives a sibling node test.
    assert!(t.css_to_xpath("*:first-of-type", "").is_err());
    assert!(t.css_to_xpath("*:last-of-type", "").is_err());
    assert!(t.css_to_xpath("*:nth-of-type(2n)", "").is_err());
    assert!(t.css_to_xpath("*:nth-last-of-type(2)", "").is_err());
    assert!(t.css_to_xpath("*:only-of-type", "").is_err());
    assert!(t.css_to_xpath(".foo:first-of-type", "").is_err());
    assert!(t.css_to_xpath("[bar]:nth-of-type(2)", "").is_err());
    assert!(t.css_to_xpath(":is(e):first-of-type", "").is_err());
    // Every wildcard subject is universal for this purpose: a
    // prefixed wildcard names a namespace, not a type, so counting
    // `ns|*` siblings would be a position among all elements in that
    // namespace rather than among elements of the same type.
    assert!(t.css_to_xpath("svg|*:first-of-type", "").is_err());
    assert!(t.css_to_xpath("svg|*:last-of-type", "").is_err());
    assert!(t.css_to_xpath("svg|*:nth-of-type(2)", "").is_err());
    assert!(t.css_to_xpath("svg|*:nth-last-of-type(2)", "").is_err());
    assert!(t.css_to_xpath("svg|*:only-of-type", "").is_err());
    assert!(t.css_to_xpath("*|*:first-of-type", "").is_err());
    assert!(t.css_to_xpath("|*:first-of-type", "").is_err());
    // :lang()/:dir() argument validation, at the level the argument
    // grammar decides it: whether the tokens assemble into ranges at
    // all. (A lone '-' is not a valid ident, so it never becomes one.)
    // A range that assembles but says something no language tag can
    // match is the translators' to reject, by name — see
    // `lang_range_errors_name_the_range`.
    assert!(t.css_to_xpath(":lang()", "").is_err());
    assert!(t.css_to_xpath(":lang(5)", "").is_err());
    assert!(t.css_to_xpath(":lang(-)", "").is_err());
    // A namespace prefix that is not an XML `NCName` cannot be a node
    // test, and XPath 1.0 cannot resolve it without the namespace URI:
    // comparing the whole `prefix:name` against `name()` would match
    // only documents using that very prefix, where the prefixes this
    // crate does emit are resolved by what the caller bound them to. So
    // the `name()` fallback a local name gets is not extended here — the
    // error is the answer, and this test is what pins that. Being an
    // `NCName` is the whole bar, so a non-ASCII prefix translates (see
    // `non_ascii_namespace_prefixes` in `names.rs`) and what is left
    // here is prefixes that are not names at all.
    let unsafe_prefix = css_to_xpath::Error::Unsupported {
        construct: "a namespace prefix that is not an XPath name (`1ns`)".to_owned(),
        offset: None,
    };
    assert_eq!(
        t.css_to_xpath("\\31 ns|div", "").unwrap_err(),
        unsafe_prefix
    );
    assert_eq!(t.css_to_xpath("\\31 ns|*", "").unwrap_err(), unsafe_prefix);
    assert_eq!(
        t.css_to_xpath("\\31 ns|di\\[v", "").unwrap_err(),
        unsafe_prefix
    );
    assert_eq!(
        t.css_to_xpath("[\\31 ns|href]", "").unwrap_err(),
        unsafe_prefix
    );
    assert_eq!(
        t.css_to_xpath("[\\31 ns|href='v']", "").unwrap_err(),
        unsafe_prefix
    );
    assert_eq!(
        t.css_to_xpath("e:is(\\31 ns|div)", "").unwrap_err(),
        unsafe_prefix
    );
    assert_eq!(
        t.css_to_xpath("e:has(> \\31 ns|div)", "").unwrap_err(),
        unsafe_prefix
    );
    // A character outside the `NCName` tables is rejected wherever it
    // sits: U+00A0 is not a name character, and U+00B7 is one but may
    // not lead.
    assert_eq!(
        t.css_to_xpath("ns\u{a0}x|div", "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "a namespace prefix that is not an XPath name (`ns\u{a0}x`)".to_owned(),
            offset: None,
        }
    );
    assert_eq!(
        t.css_to_xpath("\u{b7}ns|div", "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "a namespace prefix that is not an XPath name (`\u{b7}ns`)".to_owned(),
            offset: None,
        }
    );
    // An+B must be whitespace-exact and integer-valued.
    assert!(t.css_to_xpath("e:nth-child(3 7)", "").is_err());
    assert!(t.css_to_xpath("e:nth-child(2 n)", "").is_err());
    assert!(t.css_to_xpath("e:nth-child(2.5)", "").is_err());
    assert!(t.css_to_xpath("e:nth-child(2e1)", "").is_err());
}

/// CSS syntax errors on malformed selectors, ported from selectr's
/// parse-error suite. selectr pins its own hand-written parser's
/// exact message text; css-to-xpath parses through Servo's `selectors`
/// crate, so only the fact of an error is asserted here (message
/// wording is pinned separately in `error_messages`).
#[test]
fn parse_errors() {
    let t = Translator::new(Mode::Generic);
    // Dangling/missing selectors around commas and combinators.
    assert!(t.css_to_xpath("div, ", "").is_err());
    assert!(t.css_to_xpath(" , div", "").is_err());
    assert!(t.css_to_xpath("p, , div", "").is_err());
    assert!(t.css_to_xpath("div > ", "").is_err());
    assert!(t.css_to_xpath("  > div", "").is_err());
    assert!(t.css_to_xpath(" ", "").is_err());
    // Malformed namespace syntax.
    assert!(t.css_to_xpath("foo|#bar", "").is_err());
    assert!(t.css_to_xpath("e|", "").is_err());
    assert!(t.css_to_xpath("div .|x", "").is_err());
    // A selector cannot start with a bare '#' or ':' before a class
    // token, nor a bare '.' before a hash/pseudo token.
    assert!(t.css_to_xpath("#.foo", "").is_err());
    assert!(t.css_to_xpath(".#foo", "").is_err());
    assert!(t.css_to_xpath(":#foo", "").is_err());
    // Malformed attribute selectors.
    assert!(t.css_to_xpath("[*]", "").is_err());
    assert!(t.css_to_xpath("[foo|]", "").is_err());
    assert!(t.css_to_xpath("[#]", "").is_err());
    assert!(t.css_to_xpath("[foo=#]", "").is_err());
    assert!(t.css_to_xpath("[href]a", "").is_err());
    assert!(t.css_to_xpath("[rel:stylesheet]", "").is_err());
    // :nth-child() requires at least one argument.
    assert!(t.css_to_xpath(":nth-child()", "").is_err());
    // Stray/invalid characters.
    assert!(t.css_to_xpath("attributes(href)/html/body/a", "").is_err());
    assert!(t.css_to_xpath("attributes(href)", "").is_err());
    assert!(t.css_to_xpath("html/body/a", "").is_err());
    assert!(t.css_to_xpath("foo!", "").is_err());
    assert!(t.css_to_xpath("a[rel!=nofollow]", "").is_err());
    assert!(t.css_to_xpath("a:not(b;)", "").is_err());
    // Mis-placed pseudo-elements: not at the end of a selector, or
    // anywhere inside a functional pseudo-class's argument.
    assert!(t.css_to_xpath("a:before:empty", "").is_err());
    assert!(t.css_to_xpath("li:before a", "").is_err());
    assert!(t.css_to_xpath(":not(:before)", "").is_err());
    assert!(t.css_to_xpath(":not(a,)", "").is_err());
    assert!(t.css_to_xpath(":is(:before)", "").is_err());
    assert!(t.css_to_xpath(":matches(:before)", "").is_err());
    assert!(t.css_to_xpath(":is(a:before b)", "").is_err());
    assert!(t.css_to_xpath(":is(a b:before)", "").is_err());
    // A trailing combinator inside a functional pseudo-class's
    // argument is still a dangling combinator.
    assert!(t.css_to_xpath(":is(a >)", "").is_err());
    // The corresponding well-formed selectors are valid.
    assert!(t.css_to_xpath("[rel=stylesheet]", "").is_ok());
    assert!(t.css_to_xpath(":lang(fr)", "").is_ok());
}

/// `EmptySelector` means the group really was empty, not merely that
/// nothing in it parsed. `selectors` reports both the same way — a
/// compound that ends with no components — so a group that had a
/// token and could not use it is re-reported by that token, which is
/// what the same token one compound later already gets.
#[test]
fn empty_selector_names_what_stopped_it() {
    let t = Translator::new(Mode::Generic);
    let kind = |sel: &str| match t.css_to_xpath(sel, "").unwrap_err() {
        css_to_xpath::Error::Parse { kind, offset } => (kind, offset),
        e => panic!("{sel:?} -> {e}"),
    };

    // An identifier CSS cannot spell unescaped is named, and named the
    // same way whether or not a type selector precedes it. The caret
    // is on the offending token either way.
    for (sel, at) in [
        ("#1abc", 0),
        ("#1abc, b", 0),
        ("a, #1abc", 3),
        (":is(#1abc)", 4),
    ] {
        assert_eq!(
            kind(sel),
            (
                css_to_xpath::ParseErrorKind::UnexpectedToken("#1abc".to_owned()),
                at
            ),
            "{sel:?}"
        );
    }
    assert_eq!(
        kind("p#1abc"),
        (
            css_to_xpath::ParseErrorKind::UnexpectedToken("#1abc".to_owned()),
            1
        )
    );
    // A digit-leading class is a dimension token to the CSS
    // tokenizer, so the two spellings agree on that echo too.
    assert_eq!(kind(".1cls").0, kind("p.1cls").0);
    assert_eq!(kind("#-").0, kind("p#-").0);

    // What is left is what the name says: nothing between the commas,
    // or nothing at all.
    for (sel, at) in [("", 0), ("  ", 2), ("a,", 2), (",a", 0), ("a, , b", 3)] {
        assert_eq!(
            kind(sel),
            (css_to_xpath::ParseErrorKind::EmptySelector, at),
            "{sel:?}"
        );
    }

    // The full message, with the caret on the hash rather than on a
    // selector the reader is told is empty.
    let sel = "#1abc";
    assert_eq!(
        t.css_to_xpath(sel, "").unwrap_err().message(sel),
        "Unable to parse the CSS selector \"#1abc\": unexpected `#1abc`\n\
         \x20 |\n\
         \x20 | #1abc\n\
         \x20 | ^"
    );
}

/// When a message echoes a token, the caret is on that token.
///
/// `cssparser` and `selectors` report the position the parse stopped
/// at, which sits beside the offending token rather than on it, and on
/// whichever side depends on where the failing parser took its location
/// from: before reading a token (leaving the position on the whitespace
/// in front of it) or after (leaving it past the token, or — past a
/// function token — on that function's first argument). Both are one
/// token from what the message names, so the caret is moved onto it.
#[test]
fn caret_is_on_the_token_the_message_names() {
    let t = Translator::new(Mode::Generic);
    let err = |sel: &str| match t.css_to_xpath(sel, "").unwrap_err() {
        css_to_xpath::Error::Parse { kind, offset } => (kind, offset),
        e => panic!("{sel:?} -> {e}"),
    };
    let unexpected = |token: &str, at| {
        (
            css_to_xpath::ParseErrorKind::UnexpectedToken(token.to_owned()),
            at,
        )
    };
    let pseudo = |name: &str, at| {
        (
            css_to_xpath::ParseErrorKind::UnsupportedPseudo(name.to_owned()),
            at,
        )
    };

    // Reported before the token: an attribute selector's flags, where
    // the position lands on the space in front of the stray token.
    // Whitespace and comments are skipped over on the way to it.
    assert_eq!(err("[a=b c]"), unexpected("c", 5));
    assert_eq!(err("a[b=c d]"), unexpected("d", 6));
    assert_eq!(err("[a=b /*x*/ c]"), unexpected("c", 11));
    assert_eq!(err(":is([a=b c])"), unexpected("c", 9));

    // Reported after the token: an `An+B` argument, where the position
    // lands past what it names.
    assert_eq!(err(":nth-child(foo)"), unexpected("foo", 11));
    assert_eq!(err("a:nth-of-type(+ 2)"), unexpected(" ", 15));

    // A functional pseudo is reported on its first argument, or at the
    // end of input when it has none; either way the caret goes back to
    // the name the message quotes.
    assert_eq!(err("a::part(b)"), pseudo("part", 3));
    assert_eq!(err("a::slotted(b)"), pseudo("slotted", 3));
    assert_eq!(err("a:dir("), pseudo("dir", 2));

    // Positions already on the token they name stay put, including the
    // one selectr gets wrong the other way: `div.-5` is reported on the
    // `-5` the message echoes, not on the `.` before it. A pseudo whose
    // name is not adjacent to the reported position — `::before`, two
    // tokens along — is left alone rather than searched for.
    assert_eq!(
        err("div.-5"),
        (
            css_to_xpath::ParseErrorKind::ExpectedName("-5".to_owned()),
            4
        )
    );
    assert_eq!(err("e:frobnicate"), pseudo("frobnicate", 2));
    assert_eq!(err("a::before"), pseudo("before", 2));
    assert_eq!(err("#1abc"), unexpected("#1abc", 0));
    assert_eq!(
        err("[a~]"),
        (
            css_to_xpath::ParseErrorKind::InvalidAttributeSelector("~".to_owned()),
            2
        )
    );

    // The rendered gutter for one of each side.
    let sel = "[a=b c]";
    assert_eq!(
        t.css_to_xpath(sel, "").unwrap_err().message(sel),
        "Unable to parse the CSS selector \"[a=b c]\": unexpected `c`\n\
         \x20 |\n\
         \x20 | [a=b c]\n\
         \x20 |      ^"
    );
    let sel = ":nth-child(foo)";
    assert_eq!(
        t.css_to_xpath(sel, "").unwrap_err().message(sel),
        "Unable to parse the CSS selector \":nth-child(foo)\": unexpected `foo`\n\
         \x20 |\n\
         \x20 | :nth-child(foo)\n\
         \x20 |            ^"
    );
}

/// css-syntax-3 auto-closes open blocks, functions, and strings at
/// EOF: the parse error is flagged, not fatal, so a selector left
/// unclosed at end-of-input translates identically to its closed
/// form, in every translator mode.
#[test]
fn eof_autocloses() {
    fn eof(unclosed: &str, closed: &str) {
        for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
            let t = Translator::new(mode);
            assert_eq!(
                t.css_to_xpath(unclosed, "").unwrap(),
                t.css_to_xpath(closed, "").unwrap(),
                "{unclosed:?} vs {closed:?} in {mode:?}"
            );
        }
    }

    eof("[rel", "[rel]");
    eof("[rel=stylesheet", "[rel=stylesheet]");
    eof("[rel=stylesheet i", "[rel=stylesheet i]");
    eof("[foo=\"bar", "[foo=\"bar\"]");
    eof("[foo=\"", "[foo=\"\"]");
    eof(":lang(fr", ":lang(fr)");
    eof(":nth-child(2n+1", ":nth-child(2n+1)");
    eof(":is(a", ":is(a)");
    eof("e:is(a, b", "e:is(a, b)");
    eof(":not(a", ":not(a)");
    eof(":has(> a", ":has(> a)");
    // The unclosed string is auto-closed at parse time; the
    // pseudo-class is then rejected at translation time either way.
    let t = Translator::new(Mode::Generic);
    assert!(t.css_to_xpath(":contains(\"foo", "").is_err());
}

/// `Error::message`'s wording — including the caret-pointer gutter
/// under a `Parse` error, and the plain sentence for an
/// `Unsupported` one — is documented in `translate::error` as part
/// of the crate's output contract. Pin it here, alongside the
/// one-line `Display` form and the `Parse` vs `Unsupported` variant
/// split, which selects the message shape.
#[test]
fn error_messages() {
    let t = Translator::new(Mode::Generic);

    // A dangling combinator: not valid CSS, so `Error::Parse`. The
    // caret lands one past the last character, at the EOF offset.
    let sel = "div > ";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::DanglingCombinator,
            offset: 6
        }
    );
    assert_eq!(
        err.to_string(),
        "invalid CSS selector at byte 6: a combinator with nothing after it"
    );
    assert_eq!(
        err.message(sel),
        "Unable to parse the CSS selector \"div > \": a combinator with nothing after it\n\
         \x20 |\n\
         \x20 | div > \n\
         \x20 |       ^"
    );

    // A stray '#' where an attribute value is expected: also a
    // `Parse` error, caret under the offending character. The token
    // is echoed as the CSS it was written as, not as Servo's
    // `Debug` for it.
    let sel = "[foo=#]";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::InvalidAttributeSelector("#".to_owned()),
            offset: 5
        }
    );
    assert_eq!(
        err.to_string(),
        "invalid CSS selector at byte 5: `#` is not valid in an attribute selector"
    );
    assert_eq!(
        err.message(sel),
        "Unable to parse the CSS selector \"[foo=#]\": \
         `#` is not valid in an attribute selector\n\
         \x20 |\n\
         \x20 | [foo=#]\n\
         \x20 |      ^"
    );

    // An invalid character ('/' is not valid CSS syntax here).
    let sel = "html/body";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::UnexpectedToken("/".to_owned()),
            offset: 4
        }
    );
    assert_eq!(
        err.message(sel),
        "Unable to parse the CSS selector \"html/body\": unexpected `/`\n\
         \x20 |\n\
         \x20 | html/body\n\
         \x20 |     ^"
    );

    // An unknown pseudo-class, and a pseudo-element: the name is
    // echoed, not a `Debug` rendering of the parser's own error.
    let sel = "a:hoverr";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::UnsupportedPseudo("hoverr".to_owned()),
            offset: 2
        }
    );
    assert_eq!(
        err.to_string(),
        "invalid CSS selector at byte 2: \
         `hoverr` is not a supported pseudo-class or pseudo-element"
    );
    assert_eq!(
        t.css_to_xpath("p::before", "").unwrap_err(),
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::UnsupportedPseudo("before".to_owned()),
            offset: 2
        }
    );

    // The column combinator is valid CSS syntax but has no XPath 1.0
    // translation, so it is `Error::Unsupported`. The pre-parse scan
    // finds it by walking the source text, so this one does know where
    // it is and does render a caret.
    let sel = "col || td";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Unsupported {
            construct: "the `||` column combinator".to_owned(),
            offset: Some(4),
        }
    );
    assert_eq!(
        err.to_string(),
        "unsupported CSS construct at byte 4: the `||` column combinator"
    );
    assert_eq!(
        err.message(sel),
        "The CSS selector \"col || td\" uses the `||` column combinator, \
         which this translator does not support\n\
         \x20 |\n\
         \x20 | col || td\n\
         \x20 |     ^"
    );

    // The `&` nesting selector is the third, and the one whose
    // pre-parse catch matters most: Servo cannot begin a compound with
    // it, so the error that used to surface was whichever one the
    // parse tripped over next — a plain parse error, where the `&` is
    // valid CSS that this translator is the one refusing.
    let sel = "a:is(&)";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Unsupported {
            construct: "the `&` nesting selector".to_owned(),
            offset: Some(5),
        }
    );
    assert_eq!(
        err.to_string(),
        "unsupported CSS construct at byte 5: the `&` nesting selector"
    );
    assert_eq!(
        err.message(sel),
        "The CSS selector \"a:is(&)\" uses the `&` nesting selector, \
         which this translator does not support\n\
         \x20 |\n\
         \x20 | a:is(&)\n\
         \x20 |      ^"
    );
    // A selector with more than one problem keeps the message it had
    // before the `&` check existed: the scan reports it last.
    assert_eq!(
        t.css_to_xpath("col || &", "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "the `||` column combinator".to_owned(),
            offset: Some(4),
        }
    );

    // Excess nesting is the other construct the scan finds, and its
    // message is bounded exactly like a `Parse` one: the quoted
    // selector is elided past 120 bytes and the gutter shows a
    // 72-column window of the line around the caret, rather than all
    // 166 bytes of a selector nobody wrote by hand.
    let sel = format!("{}a{}", ":is(".repeat(33), ")".repeat(33));
    let err = t.css_to_xpath(&sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Unsupported {
            construct: "functional pseudo-classes nested more than 32 levels deep".to_owned(),
            // The 33rd `(`, one per four-byte `:is(`.
            offset: Some(33 * 4 - 1),
        }
    );
    assert_eq!(
        err.message(&sel),
        format!(
            "The CSS selector \"{}\u{2026}\" uses functional pseudo-classes nested \
             more than 32 levels deep, which this translator does not support\n\
             \x20 |\n\
             \x20 | \u{2026}{}\n\
             \x20 | {}^",
            // The quote stops at 120 bytes; the gutter window is the
            // last 72 columns of the 166-column line, which is as far
            // left as it can start while still holding the caret at
            // column 131, and the caret is 131 - 94 columns into it
            // plus one for the leading `…`.
            &sel[..120],
            &sel[94..],
            " ".repeat(38),
        )
    );

    // `:scope` inside a functional pseudo-class argument has no
    // reachable context node in an XPath 1.0 predicate. Where it is
    // supported is a lexical fact — the leftmost compound of a group,
    // and nowhere deeper than the top level — so the scan decides it
    // too, and the error carries the position the translator, handed
    // offset-free components, could not have supplied.
    let sel = "e:is(:scope)";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Unsupported {
            construct: "the `:scope` pseudo-class inside a functional pseudo-class".to_owned(),
            offset: Some(5),
        }
    );
    assert_eq!(
        err.to_string(),
        "unsupported CSS construct at byte 5: \
         the `:scope` pseudo-class inside a functional pseudo-class"
    );
    assert_eq!(
        err.message(sel),
        "The CSS selector \"e:is(:scope)\" uses the `:scope` pseudo-class \
         inside a functional pseudo-class, which this translator does not support\n\
         \x20 |\n\
         \x20 | e:is(:scope)\n\
         \x20 |      ^"
    );

    // The other misplacement is its own construct, and the caret
    // distinguishes the two `:scope`s a message otherwise could not.
    let sel = ":scope > a :scope";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Unsupported {
            construct: "the `:scope` pseudo-class outside the leftmost compound".to_owned(),
            offset: Some(11),
        }
    );
    assert_eq!(
        err.message(sel),
        "The CSS selector \":scope > a :scope\" uses the `:scope` pseudo-class \
         outside the leftmost compound, which this translator does not support\n\
         \x20 |\n\
         \x20 | :scope > a :scope\n\
         \x20 |            ^"
    );

    // Both are found after the parse, not before it, so a selector
    // that is *also* invalid CSS keeps its parse error and its caret.
    assert_eq!(
        t.css_to_xpath("a > > :scope", "").unwrap_err(),
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::DanglingCombinator,
            offset: 4,
        }
    );

    // The remaining translation-time constructs stay positionless:
    // whether an of-type pseudo-class has a type to count siblings by
    // is not a lexical fact, and locating the compound it was written
    // in would take a second model of the source that could point the
    // caret at the wrong one of several identical constructs.
    let sel = "div:first-of-type > *:first-of-type";
    let err = t.css_to_xpath(sel, "").unwrap_err();
    assert_eq!(
        err,
        css_to_xpath::Error::Unsupported {
            construct: "an of-type pseudo-class on the universal selector `*`".to_owned(),
            offset: None,
        }
    );
    assert_eq!(
        err.message(sel),
        "The CSS selector \"div:first-of-type > *:first-of-type\" uses an of-type \
         pseudo-class on the universal selector `*`, which this translator does not support"
    );
}

/// `Error` is a `std::error::Error`, so `?` converts it into the
/// boxed and `anyhow`-style error types callers actually propagate
/// through, with no wrapper of their own.
#[test]
fn error_is_std_error() {
    fn boxed() -> Result<(), Box<dyn std::error::Error>> {
        css_to_xpath::css_to_xpath("a", "", Mode::Generic)?;
        css_to_xpath::css_to_xpath("div > ", "", Mode::Generic)?;
        Ok(())
    }
    let e = boxed().unwrap_err();
    assert_eq!(
        e.to_string(),
        "invalid CSS selector at byte 6: a combinator with nothing after it"
    );
    assert!(e.source().is_none());
}

/// Neither message form echoes a dependency's `Debug` output: the
/// wording is this crate's own, so a `selectors`/`cssparser` bump
/// that renames an internal error variant cannot change it.
#[test]
fn error_messages_are_not_debug_renderings() {
    let t = Translator::new(Mode::Generic);
    for sel in [
        "div > ",
        "[foo=#]",
        "html/body",
        "p:before",
        ":is(a, :before)",
        "div.-",
        "ns|5",
        "",
        "a,",
        "a:",
        "[foo!=bar]",
        "col || td",
        "&",
        "e:is(:scope)",
    ] {
        let err = t.css_to_xpath(sel, "").unwrap_err();
        for message in [err.to_string(), err.message(sel)] {
            for debris in [
                "Delim(",
                "QuotedString(",
                "Ident(",
                "Number(",
                "InvalidState",
                "DanglingCombinator",
                "EmptySelector",
                "BadValueInAttr",
                "UnsupportedPseudoClassOrElement",
            ] {
                assert!(!message.contains(debris), "{sel:?} -> {message:?}");
            }
        }
    }
}

/// A `:lang()` range the translation cannot make sense of is reported by
/// naming the range, not merely the pseudo-class it sits in: the
/// argument grammar accepts whatever assembles into ranges, so the fault
/// is in what a range *says*, and there can be several of them.
#[test]
fn lang_range_errors_name_the_range() {
    const EMPTY_SUBTAG: &str = "an empty subtag, which a language range cannot have";
    const GLUED_WILDCARD: &str =
        "a wildcard glued to a subtag, where a language range takes one only as a whole subtag";

    // The shapes no translator can take, rejected the same way in all
    // three modes and whichever range of a list is the bad one.
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        let t = Translator::new(mode);
        for (sel, range, fault) in [
            ("e:lang(en-)", "en-", EMPTY_SUBTAG),
            ("e:lang(en--)", "en--", EMPTY_SUBTAG),
            ("e:lang(--x)", "--x", EMPTY_SUBTAG),
            ("e:lang(en, de-)", "de-", EMPTY_SUBTAG),
            ("e:lang(en*)", "en*", GLUED_WILDCARD),
            ("e:lang(*en)", "*en", GLUED_WILDCARD),
            ("e:lang(\"en-*-\")", "en-*-", EMPTY_SUBTAG),
        ] {
            assert_eq!(
                t.css_to_xpath(sel, "").unwrap_err(),
                css_to_xpath::Error::Unsupported {
                    construct: format!("the :lang() language range {range:?} ({fault})"),
                    offset: None,
                },
                "{sel} in {mode:?} mode"
            );
        }
    }

    // An interior wildcard is a well-formed range that only
    // Mode::Generic has to refuse, and its message names the range the
    // same way. The HTML modes translate it.
    let t = Translator::new(Mode::Generic);
    assert_eq!(
        t.css_to_xpath("e:lang(*-CH)", "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "the :lang() language range \"*-CH\" (a wildcard outside the final \
                        subtag, which XPath's lang() cannot express)"
                .to_owned(),
            offset: None,
        }
    );
    assert!(
        Translator::new(Mode::Html)
            .css_to_xpath("e:lang(*-CH)", "")
            .is_ok()
    );

    // Positionless, so the message is the plain sentence with no caret
    // gutter: the range reaches translation as a parsed component, which
    // no longer knows where in the selector it was written.
    let sel = "e:lang(en-)";
    assert_eq!(
        t.css_to_xpath(sel, "").unwrap_err().message(sel),
        "The CSS selector \"e:lang(en-)\" uses the :lang() language range \"en-\" \
         (an empty subtag, which a language range cannot have), \
         which this translator does not support"
    );
}

/// The payloads echoed from the selector are bounded and safe to
/// print, exactly as the quoted selector and the caret gutter are: a
/// token or pseudo-class name is elided past 40 bytes, and control
/// characters never reach the terminal raw.
#[test]
fn error_payloads_are_bounded() {
    let t = Translator::new(Mode::Generic);
    let long = "z".repeat(100);
    assert_eq!(
        t.css_to_xpath(&format!("a:{long}"), "").unwrap_err(),
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::UnsupportedPseudo(format!(
                "{}\u{2026}",
                "z".repeat(40)
            )),
            offset: 2
        }
    );
    assert_eq!(
        t.css_to_xpath("[foo=\u{1}]", "").unwrap_err(),
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::InvalidAttributeSelector("\u{fffd}".to_owned()),
            offset: 5
        }
    );
    // A `:lang()` range is echoed by the same rule, though it is not a
    // token: a range is whatever a quoted string held, which is neither
    // bounded nor printable on its own.
    assert_eq!(
        t.css_to_xpath(&format!("a:lang(\"{long}-\")"), "")
            .unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: format!(
                "the :lang() language range \"{}\u{2026}\" \
                 (an empty subtag, which a language range cannot have)",
                "z".repeat(40)
            ),
            offset: None,
        }
    );
    assert_eq!(
        t.css_to_xpath("a:lang(\"\u{1}-\")", "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "the :lang() language range \"\u{fffd}-\" \
                        (an empty subtag, which a language range cannot have)"
                .to_owned(),
            offset: None,
        }
    );
}

/// The caret gutter's alignment and bounds. The caret is padded by
/// display width, not by character or byte count, and the gutter
/// echoes a window of the *line* the error is on, so a message stays
/// legible (and printable) whatever the selector contains.
#[test]
fn error_message_caret_alignment() {
    let t = Translator::new(Mode::Generic);
    // The gutter's echo line and caret line, without the leading
    // `  | ` on each: what has to line up, isolated from the wording.
    let gutter = |sel: &str| {
        let message = t.css_to_xpath(sel, "").unwrap_err().message(sel);
        let mut lines = message
            .split('\n')
            .skip(2)
            .map(|l| l["  | ".len()..].to_owned());
        (lines.next().unwrap(), lines.next().unwrap())
    };

    // A tab is echoed as a single space: its rendered width is the
    // terminal's business, and the caret cannot guess the tab stops.
    assert_eq!(
        gutter("\tdiv >"),
        (" div >".to_owned(), "      ^".to_owned())
    );

    // Wide characters take two columns each, so the caret needs
    // eight spaces here and not the five characters (or eleven
    // bytes, or six UTF-16 units) that precede the error.
    assert_eq!(
        gutter("日本語 >"),
        ("日本語 >".to_owned(), "        ^".to_owned())
    );
    // A combining mark takes none, and a non-BMP character — two
    // UTF-16 units — still only one.
    assert_eq!(
        gutter("e\u{301}\u{ff21} >"),
        ("e\u{301}\u{ff21} >".to_owned(), "     ^".to_owned())
    );
    assert_eq!(
        gutter("\u{1f600} >"),
        ("\u{1f600} >".to_owned(), "    ^".to_owned())
    );

    // An `Unsupported` error the pre-parse scan placed renders the
    // same gutter as a `Parse` one: same padding by display width,
    // same line windowing.
    assert_eq!(
        gutter("日本語 :scope"),
        ("日本語 :scope".to_owned(), "       ^".to_owned())
    );
    // The 107-column line is windowed to its last 72 columns — 65 of
    // the 100 `a`s, then ` :scope` — and the caret is 66 columns into
    // that, plus one for the leading `…`.
    assert_eq!(
        gutter(&format!("{} :scope", "a".repeat(100))),
        (
            format!("…{} :scope", "a".repeat(65)),
            format!("{}^", " ".repeat(67))
        )
    );

    // Control characters are never echoed raw into a terminal.
    assert_eq!(
        gutter("[foo=\u{1}]"),
        ("[foo=\u{fffd}]".to_owned(), "     ^".to_owned())
    );

    // Only the line the error is on is echoed, so the caret's column
    // is a column of the text above it. All of `\n`, `\r`, `\r\n`
    // and `\f` end a line.
    for sel in ["a,\nbbbb >", "a,\rbbbb >", "a,\r\nbbbb >", "a,\u{c}bbbb >"] {
        assert_eq!(gutter(sel), ("bbbb >".to_owned(), "      ^".to_owned()));
    }

    // A line wider than the 72-column window is cut down to it,
    // centred on the caret, with `…` for what was dropped.
    let (line, caret) = gutter(&format!("{}/{}", "a".repeat(100), "b".repeat(100)));
    assert_eq!(line, format!("…{}/{}…", "a".repeat(36), "b".repeat(35)));
    assert_eq!(caret, format!("{}^", " ".repeat(37)));
    assert_eq!(line.chars().count(), 74); // 72 columns plus both `…`
    // The pad counts the leading `…` too, so the caret really is
    // under the `/` as printed.
    assert_eq!(line.chars().nth(caret.len() - 1), Some('/'));

    // Put together: a 20 KB selector still yields a message a caller
    // can print, with the caret intact — the selector is quoted only
    // as far as the 120-byte elision, and echoed only as far as the
    // window.
    let big = format!("{}div >", "a,".repeat(10_000));
    let message = t.css_to_xpath(&big, "").unwrap_err().message(&big);
    assert!(message.len() < 1024, "{} bytes", message.len());
    assert!(message.starts_with(&format!(
        "Unable to parse the CSS selector \"{}…\":",
        &big[..120]
    )));
    assert!(message.ends_with(&format!(
        "\n  | …{}div >\n  | {}^",
        "a,".repeat(33),
        " ".repeat(72)
    )));
}
