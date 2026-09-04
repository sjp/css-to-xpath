//! `:lang()` and `:dir()`, whose language source differs in all three
//! modes.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

#[test]
fn lang_and_dir() {
    /// Strip the outer `e[`…`]` predicate off a single-condition
    /// translation, to rebuild the expected OR of a comma list. The
    /// first `[` is the predicate's, whatever the element name before
    /// it, and the last `]` closes it.
    fn inner(xpath: &str) -> &str {
        let open = xpath.find('[').expect("a predicate");
        xpath[open + 1..].strip_suffix(']').expect("a closed one")
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
        // An empty *subtag* is still not a language range: `en-` reads
        // as a half-written `en-*`, not as the range `en`. (The empty
        // range itself is a different thing entirely — see below.)
        "e:lang(en-)",
        "e:lang(en--)",
        "e:lang(--x)",
        "e:lang(en,)",
        "e:lang(en,,fr)",
        // Whitespace splits a range wherever it falls, including in a
        // range that is not the first: these are `*` and `-CH`, not
        // `*-CH`.
        "e:lang(en, * -CH)",
        "e:lang(en, de- *)",
    ] {
        assert!(t.css_to_xpath(sel, "").is_err(), "{sel} should error");
    }
    // The empty range is Level 4's complement of `*`: it matches an
    // element whose language is *not* known, which is the wildcard's
    // test negated. XPath's lang() cannot express it either.
    t.check(
        "e:lang(\"\")",
        "e[not(ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0])]",
    );
    t.check(
        "e:lang(en, \"\")",
        "e[lang('en') or \
         not(ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0])]",
    );
    // A bare * stays match-anything even alongside other ranges: it
    // must not be confused with the head of an interior wildcard.
    t.check(
        "e:lang(*, fr)",
        "e[ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0] or lang('fr')]",
    );
    // HTML: nearest lang-attributed ancestor, case-folded, matched by
    // RFC 4647 extended filtering.
    let mut html = Cases::new(Mode::Html);
    html.check("e:lang(EN)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')]]");
    html.check(
        "e:lang(*)",
        "e[ancestor-or-self::*[@lang][1][string-length(@lang) > 0]]",
    );
    html.check(
        "e:lang(\"\")",
        "e[not(ancestor-or-self::*[@lang][1][string-length(@lang) > 0])]",
    );
    // A trailing wildcard matches the same prefix as the range
    // without it: both stop at a subtag boundary.
    html.check("e:lang(en-*)", html.css_to_xpath("e:lang(en)", "").unwrap());
    // The range is ASCII-lowercased, matching the XPath
    // translate() alphabet on the other side of the comparison.
    html.check("e:lang(T\u{dc}RK)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 't\u{dc}rk-')]]");
    // A multi-subtag range is RFC 4647 extended filtering: the first
    // subtag is an equality, and each later one is searched for as a
    // whole subtag in what is left after the previous match — so
    // `en-nz` matches `en-NZ` and also `en-Latn-NZ`.
    html.check("e:lang(en-nz)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-') and contains(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')), '-nz-')]]");
    // The chain extends a step per subtag, each searching the tail the
    // step before it left.
    html.check("e:lang(zh-Hant-TW)", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'zh-') and contains(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'zh-')), '-hant-') and contains(concat('-', substring-after(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'zh-')), '-hant-')), '-tw-')]]");
    // Subtags come from strings as well as idents, so each one is
    // quoted as an XPath literal rather than pasted in.
    html.check("e:lang(\"a'b-c\\\"d\")", "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), \"a'b-\") and contains(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), \"a'b-\")), '-c\"d-')]]");
    // A trailing wildcard is dropped whatever the range's length: RFC
    // 4647 skips a final `*` over whatever is left of the tag.
    html.check(
        "e:lang(en-nz-*)",
        html.css_to_xpath("e:lang(en-nz)", "").unwrap(),
    );
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
    // The empty range negates that same wildcard test, xhtml language
    // source and all.
    xhtml.check(
        "E:lang(\"\")",
        format!(
            "E[not({})]",
            inner(&xhtml.css_to_xpath("E:lang(*)", "").unwrap())
        ),
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
    // trailing wildcard matches the same range as the bare one, a
    // multi-subtag range builds the same chain over the xhtml language
    // string, and a comma list ORs.
    xhtml.check(
        "E:lang(en-*)",
        xhtml.css_to_xpath("E:lang(en)", "").unwrap(),
    );
    assert!(
        xhtml
            .css_to_xpath("E:lang(en-nz)", "")
            .unwrap()
            .ends_with("'-'), 'en-') and contains(concat('-', substring-after(concat(translate(concat(@xml:lang, substring(@lang, 1, string-length(@lang) * not(@xml:lang))), 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')), '-nz-')]]")
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
    // A wildcard is a subtag like any other, in any position: RFC 4647
    // extended filtering allows one anywhere, and the HTML modes build
    // the comparison themselves rather than handing it to XPath's
    // lang(). A leading * stands for the tag's first subtag, so the
    // walk starts one subtag in instead of anchoring with starts-with.
    html.check("e:lang(*-CH)", "e[ancestor-or-self::*[@lang][1][contains(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), '-')), '-ch-')]]");
    // ... and the chain then continues as it does after a starts-with.
    html.check("e:lang(\"*-Hant-TW\")", "e[ancestor-or-self::*[@lang][1][contains(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), '-')), '-hant-') and contains(concat('-', substring-after(concat('-', substring-after(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), '-')), '-hant-')), '-tw-')]]");
    // Only a leading wildcard spends a subtag of the tag. A later one
    // moves past nothing — every range subtag after the first is already
    // searched for through the whole remaining tag — so it drops out.
    html.check(
        "e:lang(de-*-DE)",
        html.css_to_xpath("e:lang(de-DE)", "").unwrap(),
    );
    html.check(
        "e:lang(\"de-*-*\")",
        html.css_to_xpath("e:lang(de)", "").unwrap(),
    );
    // A range of nothing but wildcards is therefore `*` itself: the tag
    // needs a first subtag for the leading one, and nothing more.
    html.check(
        "e:lang(\"*-*\")",
        html.css_to_xpath("e:lang(*)", "").unwrap(),
    );
    // The quoted spelling is the same range as the unquoted one; it is
    // also the only spelling of a wildcard next to a subtag that CSS
    // tokenizes as something other than an ident (`*-*`, `*-1996`).
    html.check(
        "e:lang(\"*-CH\")",
        html.css_to_xpath("e:lang(*-CH)", "").unwrap(),
    );
    // A range's position in the list says nothing about how it parses:
    // the whitespace after a comma ends the range before it, and the
    // range after it is assembled from adjacent tokens as usual. Both
    // orders of the same pair therefore translate, to the same two
    // conditions ORed the way they were written.
    html.check(
        "e:lang(en, *-CH)",
        format!(
            "e[{} or {}]",
            inner(&html.css_to_xpath("e:lang(en)", "").unwrap()),
            inner(&html.css_to_xpath("e:lang(*-CH)", "").unwrap()),
        ),
    );
    html.check(
        "e:lang(*-CH, en)",
        format!(
            "e[{} or {}]",
            inner(&html.css_to_xpath("e:lang(*-CH)", "").unwrap()),
            inner(&html.css_to_xpath("e:lang(en)", "").unwrap()),
        ),
    );
    // The same for a trailing wildcard, which every mode takes: the
    // generic one folds `de-*` back to `lang('de')` wherever it sits.
    t.check("e:lang(en, de-*)", "e[lang('en') or lang('de')]");
    xhtml.check(
        "E:lang(en, *-CH)",
        format!(
            "E[{} or {}]",
            inner(&xhtml.css_to_xpath("E:lang(en)", "").unwrap()),
            inner(&xhtml.css_to_xpath("E:lang(*-CH)", "").unwrap()),
        ),
    );
    // Mode::Generic hands the range to XPath's lang(), which is a prefix
    // match with nowhere to put a wildcard that is not the whole range
    // or its final subtag: an interior one errors there rather than
    // going into lang() as a literal, which would never match.
    for sel in [
        "e:lang(*-CH)",
        "e:lang(\"*-CH\")",
        "e:lang(de-*-DE)",
        "e:lang(\"de-*-DE\")",
        "e:lang(\"*-*\")",
    ] {
        assert!(t.css_to_xpath(sel, "").is_err(), "{sel} should error");
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
