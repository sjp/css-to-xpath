//! The functional pseudo-classes — `:is()`, `:not()`, `:where()` and
//! `:has()` — plus the structural and never-matching pseudo-classes.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

/// Structural pseudos and the generic never-match set.
#[test]
fn structural_and_never_match_pseudos() {
    let mut t = Cases::new(Mode::Generic);
    t.check("e:empty", "e[not(*) and not(string-length())]");
    t.check("e:EmPTY", "e[not(*) and not(string-length())]");
    t.check("e:root", "e[not(parent::*)]");
    // The generic never-match set.
    for pseudo in [
        "any-link",
        "link",
        "visited",
        "hover",
        "active",
        "focus",
        "focus-within",
        "focus-visible",
        "target",
        "target-within",
        "local-link",
        "enabled",
        "disabled",
        "checked",
        "required",
        "optional",
        "read-only",
        "read-write",
        "default",
        "placeholder-shown",
    ] {
        t.check(&format!("a:{pseudo}"), "a[0]");
    }
    t.check("a:dir(ltr)", "a[0]");
}

#[test]
fn negation_matching_where_has() {
    let mut t = Cases::new(Mode::Generic);
    t.check(
        "e:not(:nth-child(odd))",
        "e[not(count(preceding-sibling::*) mod 2 = 0)]",
    );
    t.check("e:nOT(*)", "e[0]");
    t.check("e:not(a)", "e[not(self::a)]");
    t.check(":not(*|e)", "*[not(local-name() = 'e')]");
    t.check("e:not(a, b)", "e[not(self::a or self::b)]");
    // A universal argument makes :not() unmatchable...
    t.check("div:not(a, *)", "div[0]");
    // :where() / :is() OR their arguments together into one condition
    // that ANDs with the rest of the compound.
    t.check("div:where(p)", "div[self::p]");
    t.check("div:where(p, span)", "div[self::p or self::span]");
    t.check("section:where(#main)", "section[@id = 'main']");
    t.check("input:where([required])", "input[@required]");
    t.check(
        "*:where(.highlight)",
        "*[contains(concat(' ', normalize-space(@class), ' '), ' highlight ')]",
    );
    t.check("div:where(.foo, .bar)", "div[contains(concat(' ', normalize-space(@class), ' '), ' foo ') or contains(concat(' ', normalize-space(@class), ' '), ' bar ')]");
    t.check("p:where(.highlight, #special, [data-key])", "p[contains(concat(' ', normalize-space(@class), ' '), ' highlight ') or @id = 'special' or @data-key]");
    t.check(
        "*:where(div.content)",
        "*[contains(concat(' ', normalize-space(@class), ' '), ' content ') and self::div]",
    );
    t.check("div:where(p):where(span)", "div[self::p and self::span]");
    t.check("div:is(p)", "div[self::p]");
    // :matches() is the legacy alias for :is().
    t.check("div:matches(p)", "div[self::p]");
    // ...and :is()/:where() a no-op constraint.
    t.check("e:is(*)", "e");
    t.check("div:is(a, *)", "div");
    t.check("div:where(a, *)", "div");
    // :has().
    t.check("div:has(p)", "div[.//p]");
    t.check(
        "div:has(.foo)",
        "div[.//*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]",
    );
    t.check("div:has(p, span)", "div[.//p | .//span]");
    t.check("div:has(p):has(span)", "div[.//p and .//span]");
    // `|` binds tighter than `and` in XPath 1.0, so the union needs
    // no parentheses — but it reads as though it might, so a
    // multi-argument `:has()` is parenthesized wherever it is
    // conjoined with anything else.
    t.check("div:has(p, span):has(a)", "div[(.//p | .//span) and .//a]");
    t.check(
        "div:has(p, span)[data-x]",
        "div[(.//p | .//span) and @data-x]",
    );
    // A lone one still needs none, inside brackets or inside not().
    t.check("div:not(:has(p, span))", "div[not(.//p | .//span)]");
    t.check(
        "section:has(div.content)",
        "section[.//div[contains(concat(' ', normalize-space(@class), ' '), ' content ')]]",
    );
    t.check("div:has(*)", "div[.//*]");
    t.check("section:has(#main)", "section[.//*[@id = 'main']]");
    t.check("form:has([required])", "form[.//*[@required]]");
    t.check("*:has(img)", "*[.//img]");
    // Leading combinators in :has() (selectors-4 relative selectors).
    t.check("e:has(> img)", "e[child::img]");
    t.check("e:has(~ p)", "e[following-sibling::p]");
    t.check("e:has(+ p)", "e[following-sibling::*[1][self::p]]");
    t.check("e:has(> a, ~ p)", "e[child::a | following-sibling::p]");
    t.check(
        "e:has(> .foo)",
        "e[child::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]",
    );
    t.check("e:has(+ p.foo)", "e[following-sibling::*[1][contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::p]]");
    // A trailing pseudo-class inside a leading-combinator argument is
    // a further predicate on the adjacent sibling, evaluated there — so
    // `:last-child` counts *that* element's following siblings.
    t.check(
        "e:has(+ p:last-child)",
        "e[following-sibling::*[1][count(following-sibling::*) = 0 and self::p]]",
    );
    t.check(
        "e:has(~ p:first-of-type)",
        "e[following-sibling::p[count(preceding-sibling::p) = 0]]",
    );
    // Nested :not() (Selectors Level 4).
    t.check(":not(:not(a))", "*[not(not(self::a))]");
    t.check("e:is(:not(f))", "e[not(self::f)]");
    t.check("e:has(:not(f))", "e[.//*[not(self::f)]]");
    // Prefixed names inside arguments stay node tests, resolved
    // through the namespace map like a top-level `svg|g` — not a
    // string comparison against the document's prefix.
    t.check("e:is(svg|g)", "e[self::svg:g]");
    t.check("e:not(svg|g)", "e[not(self::svg:g)]");
    t.check("e:is(svg|*)", "e[self::svg:*]");
    t.check("e:has(svg|g)", "e[.//svg:g]");
    t.check("e:has(> svg|g)", "e[child::svg:g]");
    t.check("e:has(~ svg|g)", "e[following-sibling::svg:g]");
    t.check("e:has(+ svg|g)", "e[following-sibling::*[1][self::svg:g]]");
    t.check(
        "e:has(svg|g.foo)",
        "e[.//svg:g[contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]",
    );
}

/// An empty `:is()` / `:where()` argument list. Selectors 4 makes
/// those lists forgiving, so an empty one is valid and matches
/// nothing — but nothing else about forgiveness is adopted: an
/// argument that fails to parse is still an error rather than a
/// silently dropped one.
#[test]
fn empty_forgiving_argument_lists() {
    let mut t = Cases::new(Mode::Generic);
    t.check(":is()", "*[0]");
    t.check(":where()", "*[0]");
    t.check("a:where()", "a[0]");
    t.check("e:matches()", "e[0]");
    // The name is matched case-insensitively, and the argument list
    // is empty when it holds no tokens, not only when it is empty.
    t.check(":IS( )", "*[0]");
    t.check(":is(/**/)", "*[0]");
    // Nested inside the non-forgiving pseudo-classes, whose own
    // empty argument lists stay errors.
    t.check("a:not(:is())", "a[not(0)]");
    t.check("div:has(:is())", "div[.//*[0]]");
    assert!(t.css_to_xpath(":not()", "").is_err());
    assert!(t.css_to_xpath(":has()", "").is_err());
    assert!(t.css_to_xpath(":nth-child(2 of)", "").is_err());
    // An empty *argument* is a dropped argument, not an empty list.
    assert!(t.css_to_xpath(":is(a,)", "").is_err());
    assert!(t.css_to_xpath(":is(,a)", "").is_err());
    assert!(t.css_to_xpath(":is( , )", "").is_err());
    assert!(t.css_to_xpath(":where(a,)", "").is_err());
    // An argument that fails to parse is reported, not dropped, and
    // the error names it where it stands.
    assert_eq!(
        t.css_to_xpath(":is(a, ::before)", "").unwrap_err(),
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::InvalidPosition,
            offset: 15
        }
    );
    // An empty list no longer being an error, the error reported for
    // a selector holding one is the next thing that is wrong.
    assert_eq!(
        t.css_to_xpath(":is() > ::after", "").unwrap_err(),
        css_to_xpath::Error::Parse {
            kind: css_to_xpath::ParseErrorKind::UnsupportedPseudo("after".to_owned()),
            offset: 9
        }
    );
}

/// Complex selectors (with combinators) inside the functional
/// pseudo-classes (Selectors Level 4). :is()/:where()/:not() and the
/// nth `of S` lists match their argument at the candidate element, so
/// each combinator becomes an existence test through the reversed
/// axis; :has() looks forward, extending its path compound by
/// compound.
#[test]
fn complex_pseudo_arguments() {
    let mut t = Cases::new(Mode::Generic);
    // One reversed axis per combinator.
    t.check("e:is(a b)", "e[self::b and ancestor::*[self::a]]");
    t.check("e:is(a > b)", "e[self::b and parent::*[self::a]]");
    t.check(
        "e:is(a + b)",
        "e[self::b and preceding-sibling::*[1][self::a]]",
    );
    t.check(
        "e:is(a ~ b)",
        "e[self::b and preceding-sibling::*[self::a]]",
    );
    // Longer chains nest, each step wrapping the remainder.
    t.check(
        "e:is(a b c)",
        "e[self::c and ancestor::*[self::b and ancestor::*[self::a]]]",
    );
    t.check(
        "e:is(a > b ~ c)",
        "e[self::c and preceding-sibling::*[self::b and parent::*[self::a]]]",
    );
    t.check(
        "e:is(a + b > c)",
        "e[self::c and parent::*[self::b and preceding-sibling::*[1][self::a]]]",
    );
    // :not() negates the whole chain condition; complex and compound
    // arguments OR together ('and' binds tighter than 'or').
    t.check("e:not(a b)", "e[not(self::b and ancestor::*[self::a])]");
    t.check(
        "e:not(a > b + c)",
        "e[not(self::c and preceding-sibling::*[1][self::b and parent::*[self::a]])]",
    );
    t.check(
        "e:is(a b, c)",
        "e[self::b and ancestor::*[self::a] or self::c]",
    );
    t.check(
        "e:is(a, b c)",
        "e[self::a or self::c and ancestor::*[self::b]]",
    );
    // Universal steps: a bare-`*` left-hand side is a bare axis test,
    // a bare-`*` rightmost compound leaves only the chain test, and a
    // universal *argument* still makes the list trivially true (or
    // :not() unmatchable).
    t.check("e:is(* b)", "e[self::b and ancestor::*]");
    t.check("e:is(a *)", "e[ancestor::*[self::a]]");
    t.check("e:not(a *)", "e[not(ancestor::*[self::a])]");
    t.check("e:is(a b, *)", "e");
    t.check("e:not(a b, *)", "e[0]");
    // Conditions on chain steps come before each step's name test.
    t.check(
        "e:is(a.x b.y)",
        "e[contains(concat(' ', normalize-space(@class), ' '), ' y ') and \
         self::b and \
         ancestor::*[contains(concat(' ', normalize-space(@class), ' '), ' x ') \
         and self::a]]",
    );
    t.check(
        "e:is(a[foo='bar'] > b)",
        "e[self::b and parent::*[@foo = 'bar' and self::a]]",
    );
    t.check(
        "e:is(a:first-child b)",
        "e[self::b and ancestor::*[count(preceding-sibling::*) = 0 and self::a]]",
    );
    t.check(
        "e:is(a:hover b)",
        "e[self::b and ancestor::*[0 and self::a]]",
    );
    // Nested pseudo-classes inside chain steps; an or-group condition
    // is parenthesized when conjoined with the chain test.
    t.check(
        "e:is(:not(a) b)",
        "e[self::b and ancestor::*[not(self::a)]]",
    );
    t.check(
        "e:not(:is(a b))",
        "e[not(self::b and ancestor::*[self::a])]",
    );
    t.check(
        "e:is(:not(a b) c)",
        "e[self::c and ancestor::*[not(self::b and ancestor::*[self::a])]]",
    );
    t.check(
        "e:is(:is(a, b) c)",
        "e[self::c and ancestor::*[self::a or self::b]]",
    );
    t.check(
        "e:is(c :is(a, b))",
        "e[(self::a or self::b) and ancestor::*[self::c]]",
    );
    // Prefixed names in chain steps stay self:: node tests.
    t.check("ns|e:is(a b)", "ns:e[self::b and ancestor::*[self::a]]");
    t.check("e:is(ns|a b)", "e[self::b and ancestor::*[self::ns:a]]");
    t.check("e:is(a ns|b)", "e[self::ns:b and ancestor::*[self::a]]");
    // :has() walks forward: one joiner per combinator, with the
    // leading combinator choosing the first axis.
    t.check("e:has(a b)", "e[.//a//b]");
    t.check("e:has(a > b)", "e[.//a/b]");
    t.check("e:has(a + b)", "e[.//a/following-sibling::*[1][self::b]]");
    t.check("e:has(a ~ b)", "e[.//a/following-sibling::b]");
    t.check("e:has(> a b)", "e[child::a//b]");
    t.check("e:has(> a > b)", "e[child::a/b]");
    t.check("e:has(+ a > b)", "e[following-sibling::*[1][self::a]/b]");
    t.check(
        "e:has(~ a + b)",
        "e[following-sibling::a/following-sibling::*[1][self::b]]",
    );
    t.check("e:has(~ a > b)", "e[following-sibling::a/b]");
    t.check(
        "e:has(a > b + c)",
        "e[.//a/b/following-sibling::*[1][self::c]]",
    );
    t.check(
        "e:has(> a:is(b c))",
        "e[child::a[self::c and ancestor::*[self::b]]]",
    );
    t.check(
        "e:has(a.x > b.y)",
        "e[.//a[contains(concat(' ', normalize-space(@class), ' '), ' x ')]\
         /b[contains(concat(' ', normalize-space(@class), ' '), ' y ')]]",
    );
    // Prefixed names stay path node tests, except under `+` where the
    // [1] position predicate needs the node test to stay `*`.
    t.check("e:has(ns|a > b)", "e[.//ns:a/b]");
    t.check(
        "e:has(a + ns|b)",
        "e[.//a/following-sibling::*[1][self::ns:b]]",
    );
    // `of S` with complex selectors: the chain condition filters the
    // counted siblings and constrains the current element.
    t.check(
        "e:nth-child(2n of a b)",
        "e[(count(preceding-sibling::*[self::b and ancestor::*[self::a]]) +1) \
         mod 2 = 0 and self::b and ancestor::*[self::a]]",
    );
    t.check(
        "e:nth-child(2n of a > b)",
        "e[(count(preceding-sibling::*[self::b and parent::*[self::a]]) +1) \
         mod 2 = 0 and self::b and parent::*[self::a]]",
    );
    t.check(
        "e:nth-last-child(3 of a b)",
        "e[count(following-sibling::*[self::b and ancestor::*[self::a]]) = 2 \
         and self::b and ancestor::*[self::a]]",
    );
}
