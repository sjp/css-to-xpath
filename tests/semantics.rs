//! Execution-based tests: translate a selector, evaluate the resulting
//! XPath against a fixture document, and compare the elements it selects
//! against the ones the CSS selector should match.
//!
//! These are the tests the string-pinning unit suite in `src/lib.rs`
//! cannot be: an expectation here is derived from the CSS semantics and
//! the document, not from the translator's own output, so a translation
//! that is *consistently* wrong fails.

mod common;

use common::{DC_NS, Fixture, SVG_NS, XHTML_NS};
use css_to_xpath::Mode;

/// A case is a selector and the ids it must select, in document order.
type Case<'a> = (&'a str, &'a [&'a str]);

/// Run every case before reporting, so one wrong expectation does not
/// hide the rest.
#[track_caller]
fn check(fixture: &Fixture, mode: Mode, cases: &[Case]) {
    let mut failures = String::new();
    for (css, expected) in cases {
        let got = fixture.select(css, mode);
        if got != *expected {
            failures.push_str(&format!(
                "\n  {css:?}\n    expected {expected:?}\n    got      {got:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) failed:{failures}",
        failures.matches("\n  \"").count()
    );
}

fn generic_fixture() -> Fixture {
    Fixture::new(
        include_str!("fixtures/generic.xml"),
        &[("svg", SVG_NS), ("dc", DC_NS)],
    )
}

fn xhtml_fixture() -> Fixture {
    Fixture::new(include_str!("fixtures/forms.xhtml"), &[("xhtml", XHTML_NS)])
}

fn html_fixture() -> Fixture {
    Fixture::new(include_str!("fixtures/html.xml"), &[])
}

#[test]
fn generic_type_class_and_attribute_selectors() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            ("book", &["b1", "b2", "b3", "b4"]),
            ("|book", &["b1", "b2", "b3", "b4"]),
            ("#b1", &["b1"]),
            (".hardback", &["b1", "b2"]),
            ("[class~=\"hardback\"]", &["b1", "b2"]),
            ("[title]", &["sh1"]),
            ("[data-code|=\"ab\"]", &["sh1"]),
            ("[data-code^=\"cd\"]", &["sh2"]),
            ("[data-code$=\"ef\"]", &["sh2"]),
            ("[data-code*=\"d-e\"]", &["sh2"]),
            ("[data-code=\"ab-cd\"]", &["sh1"]),
            ("shelf[class=\"nonfiction\"]", &["sh2"]),
            ("title, blank", &["ti1", "bl1", "ti2", "ti3", "ti4", "ti5"]),
        ],
    );
}

#[test]
fn generic_combinators() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            ("shelf > book", &["b1", "b2", "b3", "b4"]),
            ("book title", &["ti1", "ti2", "ti4", "ti5"]),
            ("library book", &["b1", "b2", "b3", "b4"]),
            ("book + book", &["b2"]),
            ("book ~ book", &["b2", "b3"]),
            ("magazine + book", &["b3"]),
            ("book + magazine", &["mg1"]),
            ("shelf > *", &["b1", "b2", "mg1", "b3", "sv1", "b4"]),
        ],
    );
}

#[test]
fn generic_namespaces() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            ("svg|rect", &["rc1"]),
            ("svg|*", &["sv1", "rc1"]),
            ("dc|creator", &["cr1"]),
            ("*|creator", &["cr1"]),
            ("[svg|role]", &["rc1"]),
            ("[dc|role]", &["b4"]),
            ("[*|role]", &["rc1", "b4"]),
            // An unprefixed type selector means the null namespace, so
            // it never reaches the prefixed elements.
            ("rect", &[]),
            ("creator", &[]),
        ],
    );
}

#[test]
fn generic_structural_pseudos() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            (":root", &["lib"]),
            ("book:first-child", &["b1"]),
            ("book:last-child", &["b3", "b4"]),
            ("book:first-of-type", &["b1", "b4"]),
            ("book:last-of-type", &["b3", "b4"]),
            ("book:only-of-type", &["b4"]),
            ("magazine:only-of-type", &["mg1"]),
            ("blank:empty", &["bl1"]),
            ("book:nth-child(2)", &["b2", "b4"]),
            ("title:nth-child(1)", &["ti1", "ti2", "ti3", "ti4", "ti5"]),
            ("shelf > *:nth-child(odd)", &["b1", "mg1", "sv1"]),
            ("book:nth-of-type(2)", &["b2"]),
            ("book:nth-last-of-type(1)", &["b3", "b4"]),
            ("shelf > *:nth-last-child(1)", &["b3", "b4"]),
        ],
    );
}

/// `:empty` here is Selectors 3's reading — no children at all — so an
/// element holding only whitespace does not match. Selectors 4 relaxed
/// that; see `issues/08-spec-divergences-to-fix-or-document.md`.
#[test]
fn generic_empty_does_not_match_whitespace_only_content() {
    check(&generic_fixture(), Mode::Generic, &[("spaces:empty", &[])]);
}

#[test]
fn generic_logical_pseudos() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            ("shelf:has(magazine)", &["sh1"]),
            ("shelf:has(> book)", &["sh1", "sh2"]),
            ("shelf:has(> svg|svg)", &["sh2"]),
            ("book:has(+ book)", &["b1"]),
            ("book:not(.hardback)", &["b3", "b4"]),
            ("book:is(.hardback, [xml|lang])", &["b1", "b2", "b3"]),
            ("book:where(.paperback)", &["b2"]),
            (":is(magazine, blank)", &["bl1", "mg1"]),
            ("book:nth-child(2 of book)", &["b2"]),
            ("book:nth-child(-n+2 of .hardback)", &["b1", "b2"]),
        ],
    );
}

/// `Mode::Generic` translates `:lang()` to XPath's `lang()`, which reads
/// `xml:lang` from the nearest ancestor-or-self that carries it.
#[test]
fn generic_lang() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            ("book:lang(en)", &["b1"]),
            ("book:lang(fr)", &["b3"]),
            ("title:lang(en)", &["ti1"]),
            ("title:lang(fr)", &["ti4"]),
            ("title:lang(de)", &[]),
        ],
    );
}

/// `:lang(*)` — "any known language" — is the one range XPath's `lang()`
/// cannot express, so it walks `@xml:lang` directly instead.
#[test]
fn generic_lang_wildcard_and_multiple_ranges() {
    check(
        &generic_fixture(),
        Mode::Generic,
        &[
            ("book:lang(*)", &["b1", "b3"]),
            ("title:lang(*)", &["ti1", "ti4"]),
            ("book:lang(en, fr)", &["b1", "b3"]),
            ("book:lang(en-GB)", &["b1"]),
            // XPath's lang() compares case-insensitively.
            ("book:lang(en-gb)", &["b1"]),
            ("book:lang(en-US)", &[]),
        ],
    );
}

#[test]
fn generic_scope_is_the_context_node() {
    let fixture = generic_fixture();
    assert_eq!(
        fixture.select_scoped(":scope > book", Mode::Generic, "sh1"),
        ["b1", "b2", "b3"]
    );
    assert_eq!(
        fixture.select_scoped(":scope > book", Mode::Generic, "sh2"),
        ["b4"]
    );
    assert_eq!(
        fixture.select_scoped(":scope title", Mode::Generic, "b1"),
        ["ti1"]
    );
}

#[test]
fn xhtml_form_pseudos() {
    check(
        &xhtml_fixture(),
        Mode::Xhtml,
        &[
            // fs1 is disabled; i1 sits in the *first* legend and is
            // carved out, i2 sits in the second legend and is not.
            (":disabled", &["fs1", "i2", "i3", "og1", "o1", "o2", "bt1"]),
            (
                ":enabled",
                &[
                    "i1", "fs2", "i4", "i5", "i6", "i7", "se1", "og2", "o3", "ta1",
                ],
            ),
            (":checked", &["i4", "o1"]),
            (":required", &["i6", "ta1"]),
            (":optional", &["i1", "i2", "i3", "i4", "i5", "se1"]),
            (":link", &["a1"]),
            (":any-link", &["a1"]),
            ("xhtml|fieldset:disabled", &["fs1"]),
            ("xhtml|option:checked", &["o1"]),
            ("xhtml|input:enabled", &["i1", "i4", "i5", "i6", "i7"]),
        ],
    );
}

#[test]
fn xhtml_names_are_namespaced_and_case_sensitive() {
    check(
        &xhtml_fixture(),
        Mode::Xhtml,
        &[
            ("xhtml|input", &["i1", "i2", "i3", "i4", "i5", "i6", "i7"]),
            ("xhtml|legend > xhtml|input", &["i1", "i2"]),
            (":root", &["doc"]),
            // Unprefixed means the null namespace, which no XHTML
            // element is in.
            ("input", &[]),
            // XHTML is XML: attribute values compare case-sensitively
            // unless the `i` flag asks otherwise.
            ("xhtml|input[type=\"checkbox\"]", &["i4"]),
            ("xhtml|input[type=\"CHECKBOX\"]", &[]),
            ("xhtml|input[type=\"CHECKBOX\" i]", &["i4"]),
        ],
    );
}

/// `Mode::Xhtml` reads the language from `xml:lang`, falling back to a
/// plain `lang` attribute.
#[test]
fn xhtml_lang() {
    check(
        &xhtml_fixture(),
        Mode::Xhtml,
        &[
            ("xhtml|p:lang(fr)", &["p1"]),
            ("xhtml|p:lang(de)", &["p2"]),
            ("xhtml|a:lang(en)", &["a1", "a2"]),
            ("xhtml|p:lang(en)", &[]),
        ],
    );
}

#[test]
fn html_lowercases_names_but_not_class_values() {
    check(
        &html_fixture(),
        Mode::Html,
        &[
            ("INPUT", &["i1", "i2", "i3", "i4", "i5"]),
            ("A", &["a1", "a2"]),
            ("body > p", &["p1", "p2"]),
            ("#doc", &["doc"]),
            // Class and id values stay case-sensitive.
            (".Nav", &["a1"]),
            (".nav", &["a2"]),
        ],
    );
}

/// HTML's legacy case-insensitive attribute values: `type` folds, a
/// `data-` attribute does not.
#[test]
fn html_folds_case_insensitive_attribute_values() {
    check(
        &html_fixture(),
        Mode::Html,
        &[
            ("input[type=CHECKBOX]", &["i3", "i4"]),
            ("input[type=checkbox]", &["i3", "i4"]),
            ("[TYPE=text]", &["i1", "i2"]),
            // The `s` flag turns the fold off, so the literal case
            // written in the document is what has to match.
            ("input[type=CHECKBOX s]", &["i3"]),
            ("input[type=checkbox s]", &["i4"]),
            ("input[type=TEXT s]", &["i1"]),
            ("input[type=text s]", &["i2"]),
        ],
    );
}

#[test]
fn html_form_and_link_pseudos() {
    check(
        &html_fixture(),
        Mode::Html,
        &[
            (":checked", &["i3", "i5", "o1"]),
            (":disabled", &["fs1", "i2"]),
            (":enabled", &["i1", "i3", "i4", "i5", "se1", "o1", "o2"]),
            (":link", &["a1"]),
            (":any-link", &["a1"]),
        ],
    );
}

/// `Mode::Html` reads the language from the plain `lang` attribute,
/// inherited from the nearest ancestor that has one.
#[test]
fn html_lang() {
    check(
        &html_fixture(),
        Mode::Html,
        &[
            ("p:lang(fr)", &["p1"]),
            ("p:lang(en)", &["p2"]),
            ("a:lang(en)", &["a1", "a2"]),
            ("p:lang(de)", &[]),
        ],
    );
}

/// The README claims `:disabled` and `:enabled` *partition* the element
/// set HTML defines them over. Check it against a document rather than
/// against the argument in the comments: every element with one of the
/// seven names is in exactly one of the two, and nothing else is in
/// either.
#[test]
fn disabled_and_enabled_partition_the_form_element_set() {
    const NAMES: [&str; 7] = [
        "button", "input", "select", "textarea", "optgroup", "option", "fieldset",
    ];

    for (fixture, mode, prefix) in [
        (xhtml_fixture(), Mode::Xhtml, "xhtml|"),
        (html_fixture(), Mode::Html, ""),
    ] {
        let all_names = NAMES
            .iter()
            .map(|name| format!("{prefix}{name}"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut candidates = fixture.select(&all_names, mode);
        candidates.sort();

        let disabled = fixture.select(":disabled", mode);
        let enabled = fixture.select(":enabled", mode);

        for id in &disabled {
            assert!(
                !enabled.contains(id),
                "{id} is both :disabled and :enabled in {mode:?} mode"
            );
        }

        let mut union = [disabled, enabled].concat();
        union.sort();
        assert_eq!(
            union, candidates,
            "in {mode:?} mode, :disabled and :enabled do not cover exactly {NAMES:?}"
        );
    }
}

/// The README's worked examples, evaluated rather than string-compared.
#[test]
fn readme_examples_select_what_they_claim() {
    let fixture = generic_fixture();
    assert_eq!(
        fixture.select("shelf > title", Mode::Generic),
        Vec::<String>::new()
    );
    assert_eq!(
        fixture.select("book > title", Mode::Generic),
        ["ti1", "ti2", "ti4", "ti5"]
    );
    assert_eq!(
        fixture.select("shelf > *:nth-child(odd)", Mode::Generic),
        ["b1", "mg1", "sv1"]
    );
    assert_eq!(fixture.select("book:has(> blank)", Mode::Generic), ["b1"]);
    assert_eq!(
        fixture.select(":is(library > shelf)", Mode::Generic),
        ["sh1", "sh2"]
    );
}
