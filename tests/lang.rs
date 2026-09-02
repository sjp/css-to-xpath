//! `:lang()` and `:dir()`, whose language source differs in all three
//! modes.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

#[test]
fn lang_and_dir() {
    /// Strip the outer `E[`…`]` predicate off a single-condition
    /// translation, to rebuild the expected OR of a comma list.
    fn inner(xpath: &str) -> &str {
        xpath
            .strip_prefix("E[")
            .and_then(|s| s.strip_suffix(']'))
            .unwrap()
    }

    let mut t = Cases::new(Mode::Generic);

    // Generic: XPath's lang() does prefix matching natively.
    t.check("e:lang(en)", "e[lang('en')]");
    t.check("e:lang(\"en\")", "e[lang('en')]");
    t.check("e:lang(en-*)", "e[lang('en')]");
    // A bare * matches a *known* language, which XPath's lang() cannot
    // express: walk xml:lang instead (xml:lang="" is unknown).
    t.check(
        "e:lang(*)",
        "e[ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0]]",
    );
    t.check("e:lang(en, fr)", "e[lang('en') or lang('fr')]");
    t.check(
        "e:lang(en, de, fr)",
        "e[lang('en') or lang('de') or lang('fr')]",
    );
    // Whitespace around the commas is fine.
    t.check("e:lang( en , fr )", "e[lang('en') or lang('fr')]");
    // But whitespace alone is not a separator (selectors-4 wants a
    // comma-separated list), and it does not glue a range together
    // either. `en*` is the case that matters most: read as the two
    // ranges `en` and `*`, a typo would quietly widen the selector
    // to every element with a known language.
    for sel in [
        "e:lang(en fr)",
        "e:lang(en *)",
        "e:lang(en*)",
        // Empty and empty-subtag ranges cannot match anything and
        // are not valid language ranges.
        "e:lang(\"\")",
        "e:lang(en-)",
        "e:lang(en--)",
        "e:lang(--x)",
        "e:lang(en,)",
        "e:lang(en,,fr)",
    ] {
        assert!(t.css_to_xpath(sel, "").is_err(), "{sel} should error");
    }
    // A bare * stays match-anything even alongside other ranges: it
    // must not be confused with the head of an interior wildcard.
    t.check(
        "e:lang(*, fr)",
        "e[ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0] or lang('fr')]",
    );
    // HTML: nearest lang-attributed ancestor, lowercased prefix match.
    let mut html = Cases::new(Mode::Html);
    html.check("e:lang(EN)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')]]");
    html.check(
        "e:lang(*)",
        "e[ancestor-or-self::*[@lang][1][string-length(@lang) > 0]]",
    );
    // A trailing wildcard matches the same prefix as the range
    // without it: both stop at a subtag boundary.
    html.check("e:lang(en-*)", html.css_to_xpath("e:lang(en)", "").unwrap());
    // The range is ASCII-lowercased, matching the XPath
    // translate() alphabet on the other side of the comparison.
    html.check("e:lang(T\u{dc}RK)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 't\u{dc}rk-')]]");
    // A hyphenated range keeps its full spelling in the prefix match.
    html.check("e:lang(en-nz)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-nz-')]]");
    // A comma list ORs the per-range ancestor-or-self:: tests.
    html.check("e:lang(en, fr)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')] or ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'fr-')]]");
    // xhtml shares the HTML overrides but reads either language
    // attribute: XHTML documents conventionally carry xml:lang, often
    // alongside lang, and HTML's language determination prefers
    // xml:lang when both sit on the same element. XPath 1.0 has no
    // conditional, so the lang half is truncated to zero length
    // whenever xml:lang is present.
    let mut xhtml = Cases::new(Mode::Xhtml);
    xhtml.check(
        "E:lang(*)",
        "E[ancestor-or-self::*[@xml:lang or @lang][1]\
         [string-length(concat(@xml:lang, \
         substring(@lang, 1, string-length(@lang) * not(@xml:lang)))) > 0]]",
    );
    xhtml.check(
        "E:lang(EN)",
        "E[ancestor-or-self::*[@xml:lang or @lang][1]\
         [starts-with(concat(translate(concat(@xml:lang, \
         substring(@lang, 1, string-length(@lang) * not(@xml:lang))), \
         'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), \
         '-'), 'en-')]]",
    );
    // The rest of the range handling is shared with Mode::Html: a
    // trailing wildcard matches the same prefix as the bare range, a
    // hyphenated range keeps its full spelling, and a comma list ORs.
    xhtml.check(
        "E:lang(en-*)",
        xhtml.css_to_xpath("E:lang(en)", "").unwrap(),
    );
    assert!(
        xhtml
            .css_to_xpath("E:lang(en-nz)", "")
            .unwrap()
            .ends_with("'-'), 'en-nz-')]]")
    );
    xhtml.check(
        "E:lang(en, fr)",
        format!(
            "E[{} or {}]",
            inner(&xhtml.css_to_xpath("E:lang(en)", "").unwrap()),
            inner(&xhtml.css_to_xpath("E:lang(fr)", "").unwrap()),
        ),
    );
    // Mode::Html stays lang-only: an HTML parser puts a literal
    // `xml:lang` in no namespace, which HTML ignores when determining
    // an element's language.
    assert!(
        !html
            .css_to_xpath("e:lang(en)", "")
            .unwrap()
            .contains("xml:lang")
    );
    assert!(
        !html
            .css_to_xpath("e:lang(*)", "")
            .unwrap()
            .contains("xml:lang")
    );
    // Interior wildcards (RFC 4647 extended filtering) are valid CSS
    // but inexpressible in XPath 1.0, so both spellings error rather
    // than over-match (unquoted *-CH) or never match (quoted "*-CH").
    for sel in [
        "e:lang(*-CH)",
        "e:lang(\"*-CH\")",
        "e:lang(de-*-DE)",
        "e:lang(\"de-*-DE\")",
    ] {
        assert!(t.css_to_xpath(sel, "").is_err(), "{sel} should error");
        assert!(
            html.css_to_xpath(sel, "").is_err(),
            "{sel} should error (html)"
        );
    }
    // :dir() takes exactly one identifier (selectors-4) — none of
    // :lang()'s strings, wildcards, or lists. It never matches in any
    // translator: resolved directionality needs runtime bidi
    // resolution, and a nearest-@dir approximation is deliberately
    // not attempted (see apply_pseudo_class).
    t.check("e:dir(rtl)", "e[0]");
    html.check("e:dir(rtl)", "e[0]");
    xhtml.check("e:dir(ltr)", "e[0]");
    // Never-match applies regardless of the (valid) ident's value.
    t.check("e:dir(foo)", "e[0]");
    assert!(t.css_to_xpath("e:dir()", "").is_err());
    assert!(t.css_to_xpath("e:dir(ltr rtl)", "").is_err());
    assert!(t.css_to_xpath("e:dir(ltr, rtl)", "").is_err());
    assert!(t.css_to_xpath("e:dir(\"ltr\")", "").is_err());
    assert!(t.css_to_xpath("e:dir(*)", "").is_err());
}
