//! Execution-based tests: translate a selector, evaluate the resulting
//! XPath against a fixture document, and compare the elements it selects
//! against the ones the CSS selector should match.
//!
//! These are the tests the string-pinning suites cannot be: an
//! expectation here is derived from the CSS semantics and
//! the document, not from the translator's own output, so a translation
//! that is *consistently* wrong fails.

mod common;

use common::{DC_NS, Fixture, SVG_NS, XHTML_NS};
use css_to_xpath::{Mode, Translator};

/// A case is a selector and the ids it must select, in document order.
type Case<'a> = (&'a str, &'a [&'a str]);

/// Run every case before reporting, so one wrong expectation does not
/// hide the rest.
#[track_caller]
fn check(fixture: &Fixture, mode: Mode, cases: &[Case]) {
    check_with(fixture, &Translator::new(mode), cases);
}

/// [`check`] with a configured translator, for the cases a bare [`Mode`]
/// cannot express.
#[track_caller]
fn check_with(fixture: &Fixture, translator: &Translator, cases: &[Case]) {
    let mut failures = String::new();
    for (css, expected) in cases {
        let got = fixture.select_with(css, translator);
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

/// The form-state fixture. It carries no mixed-case attribute value and
/// no namespace, which is exactly what `Mode::Html` and `Mode::Xhtml`
/// differ over, so every case below is run in both modes.
fn editable_fixture() -> Fixture {
    Fixture::new(include_str!("fixtures/editable.xml"), &[])
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
            // An element without the attribute is never matched by a
            // substring operator, which is why none of them tests for
            // the attribute's existence first.
            ("[title^=\"Fic\"]", &["sh1"]),
            ("[title$=\"ion\"]", &["sh1"]),
            ("[title*=\"icti\"]", &["sh1"]),
            ("[title~=\"Fiction\"]", &["sh1"]),
            ("[title|=\"Fiction\"]", &["sh1"]),
            // A value carrying both quote kinds goes through concat().
            ("[data-note=\"it's \\\"q\\\"\"]", &["mg1"]),
            ("[data-note*=\"'s \\\"q\\\"\"]", &["mg1"]),
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
            // `+ *` needs no self:: test, so check it still means "the
            // very next sibling, whatever its name".
            ("book + *", &["b2", "mg1"]),
            ("magazine + *", &["b3"]),
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
            // An+B with no solution: `0`, not a negative sibling count.
            ("book:nth-child(0)", &[]),
            ("book:nth-last-child(0)", &[]),
            ("book:nth-of-type(0)", &[]),
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
            // A multi-argument `:has()` renders as a union, which is
            // parenthesized once anything else is AND-ed onto it.
            ("shelf:has(magazine, svg|rect)", &["sh1", "sh2"]),
            ("shelf:has(magazine, svg|rect):has(> book)", &["sh1", "sh2"]),
            ("shelf:has(magazine, svg|rect)[data-code^=\"cd\"]", &["sh2"]),
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
            // A named subject prunes the overrides down to the arm for
            // that name — including the optgroup rules, which differ for
            // `option` and for `optgroup` itself.
            ("xhtml|option:disabled", &["o1", "o2"]),
            ("xhtml|optgroup:disabled", &["og1"]),
            ("xhtml|optgroup:enabled", &["og2"]),
            ("xhtml|input:disabled", &["i2", "i3"]),
            ("xhtml|button:disabled", &["bt1"]),
            ("xhtml|input:required", &["i6"]),
            ("xhtml|textarea:required", &["ta1"]),
            // `required` has no effect on a hidden input, so i7 is
            // neither :required nor :optional.
            ("xhtml|input:optional", &["i1", "i2", "i3", "i4", "i5"]),
            ("xhtml|a:link", &["a1"]),
            ("xhtml|p:link", &[]),
            ("xhtml|p:checked", &[]),
            ("xhtml|a:enabled", &[]),
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

/// A default namespace prefix is the answer to the case above: the
/// unprefixed names a caller actually writes select the XHTML elements,
/// with no `xhtml|` on every step and no `local-name()` test.
#[test]
fn xhtml_default_namespace_makes_unprefixed_names_match() {
    check_with(
        &xhtml_fixture(),
        &Translator::new(Mode::Xhtml).with_default_namespace_prefix("xhtml"),
        &[
            ("input", &["i1", "i2", "i3", "i4", "i5", "i6", "i7"]),
            ("legend > input", &["i1", "i2"]),
            ("body > p", &["p1", "p2"]),
            ("body > *", &["f1", "a1", "a2", "p1", "p2"]),
            ("fieldset:nth-of-type(2)", &["fs2"]),
            (
                "select :is(option, optgroup)",
                &["og1", "o1", "o2", "og2", "o3"],
            ),
            ("input:required", &["i6"]),
            ("option:checked", &["o1"]),
            ("a:link", &["a1"]),
            ("p:lang(fr)", &["p1"]),
            ("input[type=\"checkbox\"]", &["i4"]),
            (":root", &["doc"]),
            // The written forms keep their own meanings: `|e` is still
            // the null namespace, and `*|e` still any namespace.
            ("|input", &[]),
            ("*|input", &["i1", "i2", "i3", "i4", "i5", "i6", "i7"]),
            ("xhtml|input", &["i1", "i2", "i3", "i4", "i5", "i6", "i7"]),
        ],
    );
}

/// The default namespace is a *constraint*, not a fallback: in a
/// document whose elements are mostly outside it, an unprefixed name
/// and the implicit universal of a type-less compound match only what
/// is inside it.
#[test]
fn default_namespace_constrains_the_implicit_universal() {
    check_with(
        &generic_fixture(),
        // The fixture's own elements are in no namespace, so an SVG
        // default namespace leaves only the two `svg:` elements in it.
        &Translator::new(Mode::Generic).with_default_namespace_prefix("svg"),
        &[
            ("*", &["sv1", "rc1"]),
            ("[id]", &["sv1", "rc1"]),
            ("svg", &["sv1"]),
            ("svg > rect", &["rc1"]),
            ("[svg|role]", &["rc1"]),
            ("book", &[]),
            (":root", &[]),
            // `|e` names the null namespace whatever the default is,
            // and `*|e` every namespace.
            ("|book", &["b1", "b2", "b3", "b4"]),
            ("|library > |shelf", &["sh1", "sh2"]),
            ("*|rect", &["rc1"]),
            ("dc|creator", &["cr1"]),
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
            ("INPUT", &["i1", "i2", "i3", "i4", "i5", "i6"]),
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
            (
                ":enabled",
                &["i1", "i3", "i4", "i5", "i6", "se1", "o1", "o2"],
            ),
            (":link", &["a1"]),
            (":any-link", &["a1"]),
            // Naming the element lets the translation drop the arms
            // written for the other names; it must still select the same
            // elements as the wildcard form restricted to that name.
            ("input:checked", &["i3", "i5"]),
            ("option:checked", &["o1"]),
            ("p:checked", &[]),
            ("input:disabled", &["i2"]),
            ("fieldset:disabled", &["fs1"]),
            ("option:enabled", &["o1", "o2"]),
            ("select:enabled", &["se1"]),
            ("a:link", &["a1"]),
            ("p:link", &[]),
            // `a` is in neither the :enabled nor the :disabled set.
            ("a:enabled", &[]),
            ("a:disabled", &[]),
            // `required` applies to a text input but not a checkbox, and
            // to no element outside input/select/textarea.
            ("input:optional", &["i1", "i2", "i3", "i4", "i5"]),
            // i6's `type` is not one of the seven the `required`
            // attribute is inert on, even though it contains two of
            // them separated by the delimiter the test uses.
            ("input:required", &["i6"]),
            ("p:optional", &[]),
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

/// `:read-only`/`:read-write`, `:default` and `:placeholder-shown` over
/// a document arranged around their corners.
#[test]
fn form_state_pseudos() {
    let fixture = editable_fixture();
    let cases: &[Case] = &[
        // i1 is disabled by its fieldset and i3 is readonly; `readonly`
        // does not apply to a checkbox (i4), so it is read-only too,
        // while a missing (i5) or invalid (i6) type is the Text state,
        // which it does apply to.
        ("input:read-write", &["i2", "i5", "i6", "i11", "i12", "i13"]),
        (
            "input:read-only",
            &["i1", "i3", "i4", "i7", "i8", "i9", "i10", "i14"],
        ),
        ("textarea:read-write", &["ta1", "ta3", "ta4"]),
        ("textarea:read-only", &["ta2"]),
        // Editability is not a form-control property: everything in a
        // contenteditable subtree is read-write, and a `false` island
        // inside one is not.
        ("div:read-write", &["d1", "d3", "d4"]),
        ("div:read-only", &["d2"]),
        ("p:read-write", &["p1", "p3", "p4"]),
        ("p:read-only", &["p2"]),
        (
            ":read-write",
            &[
                "i2", "i5", "i6", "ta1", "d1", "p1", "d3", "p3", "d4", "p4", "i11", "i12", "i13",
                "ta3", "ta4",
            ],
        ),
        // f1's default button is bt1 — a `button` with no type is a
        // submit button, and it comes before the submit input i7. f2's
        // is i9: bt2 and i8 are buttons but not submit ones. bt3 has no
        // form owner, so it is no form's default button.
        (":default", &["bt1", "i9", "i10", "o2"]),
        ("button:default", &["bt1"]),
        ("input:default", &["i9", "i10"]),
        ("option:default", &["o2"]),
        ("p:default", &[]),
        // i12 has a value, i13's placeholder is empty, and `placeholder`
        // does not apply to a checkbox (i14); ta4 has text content,
        // which is a textarea's value.
        (":placeholder-shown", &["i11", "ta3"]),
        ("input:placeholder-shown", &["i11"]),
        ("textarea:placeholder-shown", &["ta3"]),
    ];
    for mode in [Mode::Html, Mode::Xhtml] {
        check(&fixture, mode, cases);
    }
}

/// Selectors 4 defines `:read-only` as the complement of `:read-write`,
/// so unlike `:disabled`/`:enabled` the two partition *every* element of
/// a document, not a named subset of them. Checked against the documents
/// rather than against the argument in the comments.
#[test]
fn read_only_and_read_write_partition_every_element() {
    for (fixture, mode) in [
        (editable_fixture(), Mode::Html),
        (editable_fixture(), Mode::Xhtml),
        (html_fixture(), Mode::Html),
        (xhtml_fixture(), Mode::Xhtml),
    ] {
        let mut everything = fixture.select("*", mode);
        everything.sort();
        assert!(!everything.is_empty(), "the fixture selected no elements");

        let read_only = fixture.select(":read-only", mode);
        let read_write = fixture.select(":read-write", mode);
        for id in &read_write {
            assert!(
                !read_only.contains(id),
                "{id} is both :read-only and :read-write in {mode:?} mode"
            );
        }

        let mut union = [read_only, read_write].concat();
        union.sort();
        assert_eq!(
            union, everything,
            "in {mode:?} mode, :read-only and :read-write do not cover every element"
        );
    }
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

        // Naming the subject lets the translation prune the overrides to
        // the arms that can apply to that name; the result must agree
        // with the wildcard form filtered to elements of that name.
        for name in NAMES {
            let of_name = fixture.select(&format!("{prefix}{name}"), mode);
            for pseudo in [":disabled", ":enabled"] {
                let named = fixture.select(&format!("{prefix}{name}{pseudo}"), mode);
                let wildcard: Vec<String> = fixture
                    .select(pseudo, mode)
                    .into_iter()
                    .filter(|id| of_name.contains(id))
                    .collect();
                assert_eq!(
                    named, wildcard,
                    "in {mode:?} mode, {prefix}{name}{pseudo} disagrees with {pseudo}"
                );
            }
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
