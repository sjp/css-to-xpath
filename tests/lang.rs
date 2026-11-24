//! `:lang()` and `:dir()`, whose language source differs in all three
//! modes.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

/// The HTML language string, ASCII-lowercased and dash-terminated.
const LC: &str = "concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                  'abcdefghijklmnopqrstuvwxyz'), '-')";

/// The `xml:lang` language string, ASCII-lowercased and dash-terminated.
const XLC: &str = "concat(translate(@xml:lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                   'abcdefghijklmnopqrstuvwxyz'), '-')";

/// `tail` cut just after its first singleton subtag: every letter and
/// digit maps to `a`, so a singleton reads `-a-`.
fn cut(tail: &str) -> String {
    format!(
        "substring({tail}, 1, string-length(substring-before(concat(translate(concat('-', {tail}), \
         'abcdefghijklmnopqrstuvwxyz0123456789', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'), 'a-'), \
         '-a-')) + 2)"
    )
}

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
    // Conjoined with another condition, only a list is a disjunction
    // that needs parentheses.
    const X: &str = "contains(concat(' ', normalize-space(@class), ' '), ' x ')";
    t.check("e:lang(en).x", format!("e[lang('en') and {X}]"));
    t.check(
        "e:lang(en, fr).x",
        format!("e[(lang('en') or lang('fr')) and {X}]"),
    );
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
    // The search stops at the tag's first singleton, though: in
    // `en-x-nz` the `nz` is private-use data, not a region.
    html.check(
        "e:lang(en-nz)",
        format!(
            "e[ancestor-or-self::*[@lang][1][starts-with({LC}, 'en-') and \
             contains(concat('-', {}), '-nz-')]]",
            cut(&format!("substring-after({LC}, 'en-')")),
        ),
    );
    // The chain extends a step per subtag, each searching the cut tail
    // the step before it left.
    let zh = cut(&format!("substring-after({LC}, 'zh-')"));
    html.check(
        "e:lang(zh-Hant-TW)",
        format!(
            "e[ancestor-or-self::*[@lang][1][starts-with({LC}, 'zh-') and \
             contains(concat('-', {zh}), '-hant-') and \
             contains(concat('-', substring-after(concat('-', {zh}), '-hant-')), '-tw-')]]"
        ),
    );
    // A singleton in the range matches the tag's singleton, and the
    // tail after it is cut afresh at the next one.
    let de = format!("substring-after({LC}, 'de-')");
    html.check(
        "e:lang(de-x-foo)",
        format!(
            "e[ancestor-or-self::*[@lang][1][starts-with({LC}, 'de-') and \
             contains(concat('-', {}), '-x-') and \
             contains(concat('-', {}), '-foo-')]]",
            cut(&de),
            cut(&format!("substring-after(concat('-', {de}), '-x-')")),
        ),
    );
    // Subtags come from strings as well as idents, so each one is
    // quoted as an XPath literal rather than pasted in.
    html.check(
        "e:lang(\"a'b-c\\\"d\")",
        format!(
            "e[ancestor-or-self::*[@lang][1][starts-with({LC}, \"a'b-\") and \
             contains(concat('-', {}), '-c\"d-')]]",
            cut(&format!("substring-after({LC}, \"a'b-\")")),
        ),
    );
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
            .ends_with(&format!(
                "'-'), 'en-') and contains(concat('-', {}), '-nz-')]]",
                cut("substring-after(concat(translate(concat(@xml:lang, \
                     substring(@lang, 1, string-length(@lang) * not(@xml:lang))), \
                     'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')"),
            ))
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
    let any = cut(&format!("substring-after({LC}, '-')"));
    html.check(
        "e:lang(*-CH)",
        format!("e[ancestor-or-self::*[@lang][1][contains(concat('-', {any}), '-ch-')]]"),
    );
    // ... and the chain then continues as it does after a starts-with.
    html.check(
        "e:lang(\"*-Hant-TW\")",
        format!(
            "e[ancestor-or-self::*[@lang][1][contains(concat('-', {any}), '-hant-') and \
             contains(concat('-', substring-after(concat('-', {any}), '-hant-')), '-tw-')]]"
        ),
    );
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
    // Mode::Generic matches any range that lang() cannot express (more
    // than one subtag, or a wildcard before the last) by the same
    // extended filtering, over xml:lang.
    t.check(
        "e:lang(en-nz)",
        format!(
            "e[ancestor-or-self::*[@xml:lang][1][starts-with({XLC}, 'en-') and \
             contains(concat('-', {}), '-nz-')]]",
            cut(&format!("substring-after({XLC}, 'en-')")),
        ),
    );
    let any = cut(&format!("substring-after({XLC}, '-')"));
    let star_ch =
        format!("e[ancestor-or-self::*[@xml:lang][1][contains(concat('-', {any}), '-ch-')]]");
    t.check("e:lang(*-CH)", &star_ch);
    t.check("e:lang(\"*-CH\")", &star_ch);
    t.check("e:lang(de-*-DE)", t.xpath("e:lang(de-DE)"));
    t.check("e:lang(\"*-*\")", t.xpath("e:lang(*)"));
    // A range that is a single subtag once its later wildcards drop out
    // is still lang().
    t.check("e:lang(\"de-*-*\")", "e[lang('de')]");
    t.check(
        "e:lang(en, *-CH)",
        format!("e[lang('en') or {}]", inner(&star_ch)),
    );
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
