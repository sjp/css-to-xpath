//! Differential test for the HTML modes: the translated XPath must
//! select exactly the elements Servo's own matcher selects, over a
//! document whose HTML pseudo-classes actually have something to say.
//!
//! `tests/differential.rs` does this for `Mode::Generic`, where the
//! translation is a pure function of the tree and the reference has no
//! HTML to model. That leaves the modes with the most logic in them —
//! `:disabled`'s fieldset walk, `:required`'s inert types, `:read-write`'s
//! `contenteditable` resolution, `:default`'s form-owner walk — pinned
//! only by cases somebody thought to write. This suite generates them
//! instead.
//!
//! # Why the reference is a second reading, not a mirror
//!
//! The reference (`tests/reference/mod.rs`, shared with the generic
//! suite) answers these pseudo-classes from the HTML standard's own
//! definitions: the "actually disabled" concept, the attribute tables'
//! "Applies to" rows, the form owner and default button algorithms. A
//! reference transcribed from `src/translate/pseudo.rs` would agree
//! with it for free; two readings of the same spec agreeing is the
//! signal worth having. Where the crate documents an approximation that
//! no document can settle — `:checked` reading attributes rather than
//! checkedness, `:placeholder-shown` answering for the initial value,
//! `:default` taking the form owner to be the nearest ancestor `form` —
//! the reference is given the same approximation explicitly, named
//! where it is made.
//!
//! # `Mode::Xhtml`, and why that covers `Mode::Html` too
//!
//! The pseudo-class translations are shared by both HTML modes; the
//! modes differ only in element- and attribute-name lowercasing, in
//! whether HTML's legacy case-insensitive attribute *values* fold, and
//! in where `:lang()` reads a language from. The grammar below writes
//! every name in lower case, marks every attribute-value comparison
//! `s`, and generates no `:lang()` — so the two modes emit the same
//! XPath for everything it produces, which [`Env::answers`] asserts on
//! every case. `Mode::Xhtml` is the one evaluated, and the assertion is
//! what makes the answer `Mode::Html`'s as well.
//!
//! # What the grammar does not generate
//!
//! - **`:lang()`.** Extended filtering is the one place the crate
//!   knowingly diverges from the spec (RFC 4647's singleton rule), so a
//!   reference for it would encode the divergence rather than check it.
//!   `tests/lang.rs` and `tests/semantics.rs` pin it by hand instead.
//! - **The of-type pseudo-classes**, exhausted by the generic suite over
//!   a fixture built to discriminate them; here every element is in one
//!   namespace, so they would add nothing.
//! - **`:has()` inside `:has()`**, which the translator rejects, and the
//!   pseudo-classes whose answer is not in the tree (`:hover`,
//!   `:visited`), which the reference cannot answer.

mod common;
mod reference;

use css_to_xpath::{Mode, Translator};
use proptest::prelude::*;

use common::{Fixture, XHTML_NS};
use reference::Reference;

/// The prefix bindings the fixture declares, shared by the XPath
/// evaluation context and the reference matcher's parser.
const NAMESPACES: &[(&str, &str)] = &[("xhtml", XHTML_NS)];

/// The fixture, and the reference matcher over the very same tree.
struct Env {
    fixture: Fixture,
    reference: Reference,
}

impl Env {
    fn new() -> Self {
        Env::from_xml(include_str!("fixtures/differential-html.xhtml"), NAMESPACES)
    }

    fn from_xml(xml: &'static str, namespaces: &'static [(&'static str, &'static str)]) -> Self {
        let fixture = Fixture::new(xml, namespaces);
        let reference = Reference::new_html(fixture.document(), namespaces);
        Env { fixture, reference }
    }

    /// The two answers for `css`: what the translated XPath selects
    /// under `Mode::Xhtml`, and what Servo's matcher selects.
    ///
    /// Every selector is also required to translate identically under
    /// `Mode::Html` — see the module comment: that is what extends the
    /// comparison to the other HTML mode without a second fixture in a
    /// second document flavour.
    fn answers(&self, css: &str) -> (Vec<String>, Vec<String>) {
        assert_modes_agree(css);
        let translated = self.fixture.select(css, Mode::Xhtml);
        let reference = self
            .reference
            .select(css)
            .unwrap_or_else(|e| panic!("the generated selector is not valid CSS: {e}"));
        (translated, reference)
    }
}

/// The two HTML modes must emit the same XPath for a selector the
/// grammar produces. A failure here means the grammar has grown a shape
/// the modes disagree about — a mixed-case name, an unflagged
/// comparison against one of HTML's legacy attributes, `:lang()` — and
/// so that `Mode::Html`'s answer is no longer the one checked below.
#[track_caller]
fn assert_modes_agree(css: &str) {
    const PREFIX: &str = "descendant-or-self::";
    let xhtml = Translator::new(Mode::Xhtml).css_to_xpath(css, PREFIX);
    let html = Translator::new(Mode::Html).css_to_xpath(css, PREFIX);
    assert_eq!(xhtml, html, "the HTML modes translate {css:?} differently");
}

// Parsing the fixture and indexing it costs more than a single case, so
// both are done once. `Fixture` owns a non-`Sync` `Package`, hence a
// thread-local rather than a `OnceLock`.
thread_local! {
    static ENV: Env = Env::new();
}

/// The HTML pseudo-classes whose answer is wholly in the document tree,
/// which is the whole set this suite exists for.
const PSEUDOS: &[&str] = &[
    ":disabled",
    ":enabled",
    ":checked",
    ":required",
    ":optional",
    ":read-only",
    ":read-write",
    ":default",
    ":placeholder-shown",
    ":link",
    ":any-link",
];

/// Element names to write as type selectors: the ones each pseudo-class
/// singles out, plus names outside every one of those sets, where a
/// pinned name collapses the translation to "never matches".
const TYPES: &[&str] = &[
    "input", "textarea", "select", "option", "optgroup", "button", "fieldset", "legend", "form",
    "a", "area", "div", "p", "link",
];

const CLASSES: &[&str] = &["one", "two", "three", "four"];
const IDS: &[&str] = &["i20", "d1", "bt3", "nope"];

/// Attributes to test for existence. Every one of them is an attribute
/// some pseudo-class reads, so an attribute selector alongside a
/// pseudo-class narrows to the interesting elements rather than to none.
const ATTRS: &[&str] = &[
    "disabled",
    "required",
    "readonly",
    "checked",
    "selected",
    "placeholder",
    "contenteditable",
    "href",
    "value",
    "type",
    "class",
];

/// Values for the `[type="…" s]` comparisons. The `s` flag is what keeps
/// the two HTML modes' output identical: `type` is on HTML's legacy
/// case-insensitive list, so an unflagged comparison folds under
/// `Mode::Html` and not under `Mode::Xhtml`.
const TYPE_VALUES: &[&str] = &["text", "checkbox", "submit", "hidden", "RANGE", "wat", ""];

/// The type-selector part of a compound. `Implicit`, `Universal` and
/// `NsUniversal` leave the subject's local name unpinned, which is the
/// translation's full-expression path; the two named forms pin it,
/// which is the path that folds every arm but one.
#[derive(Clone, Debug)]
enum Ty {
    /// No type selector at all: the compound is conditions only.
    Implicit,
    /// `*`
    Universal,
    /// `xhtml|*`
    NsUniversal,
    /// `*|e`
    AnyNsNamed(&'static str),
    /// `xhtml|e`
    NsNamed(&'static str),
}

impl Ty {
    fn render(&self) -> String {
        match *self {
            Ty::Implicit => String::new(),
            Ty::Universal => "*".to_owned(),
            Ty::NsUniversal => "xhtml|*".to_owned(),
            Ty::AnyNsNamed(name) => format!("*|{name}"),
            Ty::NsNamed(name) => format!("xhtml|{name}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum NthTy {
    Child,
    LastChild,
    OnlyChild,
}

#[derive(Clone, Copy, Debug)]
enum Comb {
    Descendant,
    Child,
    NextSibling,
    LaterSibling,
}

impl Comb {
    fn render(self) -> &'static str {
        match self {
            Comb::Descendant => " ",
            Comb::Child => " > ",
            Comb::NextSibling => " + ",
            Comb::LaterSibling => " ~ ",
        }
    }
}

#[derive(Clone, Debug)]
enum Simple {
    /// One of [`PSEUDOS`], written as it appears there.
    Pseudo(&'static str),
    Class(&'static str),
    Id(&'static str),
    AttrExists(&'static str),
    /// `[type="…" s]`; see [`TYPE_VALUES`] for the flag.
    TypeEquals(&'static str),
    Nth {
        ty: NthTy,
        a: i32,
        b: i32,
        /// The Level 4 `of S` argument, valid on the child pseudos only.
        of: Option<Vec<Complex>>,
    },
    Root,
    Empty,
    Not(Vec<Complex>),
    Is(Vec<Complex>),
    /// `:has()`, whose arguments are relative selectors: an optional
    /// leading combinator and a complex selector.
    Has(Vec<(Option<Comb>, Complex)>),
}

#[derive(Clone, Debug)]
struct Compound {
    ty: Ty,
    simples: Vec<Simple>,
}

#[derive(Clone, Debug)]
struct Complex {
    head: Compound,
    tail: Vec<(Comb, Compound)>,
}

/// `An+B`, with `b` always explicitly signed so the two halves cannot
/// run together.
fn render_an_plus_b(a: i32, b: i32) -> String {
    if b < 0 {
        format!("{a}n-{}", b.unsigned_abs())
    } else {
        format!("{a}n+{b}")
    }
}

impl Simple {
    fn render(&self) -> String {
        match self {
            Simple::Pseudo(name) => (*name).to_owned(),
            Simple::Class(name) => format!(".{name}"),
            Simple::Id(id) => format!("#{id}"),
            Simple::AttrExists(name) => format!("[{name}]"),
            Simple::TypeEquals(value) => format!("[type=\"{value}\" s]"),
            Simple::Nth { ty, a, b, of } => {
                let of = of
                    .as_ref()
                    .map_or_else(String::new, |list| format!(" of {}", render_list(list)));
                let an_plus_b = render_an_plus_b(*a, *b);
                match ty {
                    NthTy::Child => format!(":nth-child({an_plus_b}{of})"),
                    NthTy::LastChild => format!(":nth-last-child({an_plus_b}{of})"),
                    NthTy::OnlyChild => ":only-child".to_owned(),
                }
            }
            Simple::Root => ":root".to_owned(),
            Simple::Empty => ":empty".to_owned(),
            Simple::Not(list) => format!(":not({})", render_list(list)),
            Simple::Is(list) => format!(":is({})", render_list(list)),
            Simple::Has(relatives) => {
                let args: Vec<String> = relatives
                    .iter()
                    .map(|(combinator, complex)| {
                        let lead = match combinator {
                            None | Some(Comb::Descendant) => "",
                            Some(Comb::Child) => "> ",
                            Some(Comb::NextSibling) => "+ ",
                            Some(Comb::LaterSibling) => "~ ",
                        };
                        format!("{lead}{}", render_complex(complex))
                    })
                    .collect();
                format!(":has({})", args.join(", "))
            }
        }
    }
}

fn render_compound(compound: &Compound) -> String {
    // A compound of nothing at all is not a selector; `*` is what an
    // implicit type means anyway.
    if matches!(compound.ty, Ty::Implicit) && compound.simples.is_empty() {
        return "*".to_owned();
    }
    let mut out = compound.ty.render();
    for simple in &compound.simples {
        out.push_str(&simple.render());
    }
    out
}

fn render_complex(complex: &Complex) -> String {
    let mut out = render_compound(&complex.head);
    for (combinator, compound) in &complex.tail {
        out.push_str(combinator.render());
        out.push_str(&render_compound(compound));
    }
    out
}

fn render_list(list: &[Complex]) -> String {
    list.iter()
        .map(render_complex)
        .collect::<Vec<_>>()
        .join(", ")
}

fn ty_strategy() -> impl Strategy<Value = Ty> {
    let name = || prop::sample::select(TYPES);
    prop_oneof![
        4 => Just(Ty::Implicit),
        2 => Just(Ty::Universal),
        1 => Just(Ty::NsUniversal),
        3 => name().prop_map(Ty::AnyNsNamed),
        4 => name().prop_map(Ty::NsNamed),
    ]
}

fn comb_strategy() -> impl Strategy<Value = Comb> {
    prop_oneof![
        Just(Comb::Descendant),
        Just(Comb::Child),
        Just(Comb::NextSibling),
        Just(Comb::LaterSibling),
    ]
}

/// `of S` doubles the translated output per nesting level, and the HTML
/// pseudo-classes are the longest fragments the translator emits, so its
/// argument is always generated at depth 0 and only rarely at all.
fn nth_strategy(depth: u32) -> impl Strategy<Value = Simple> {
    let ty = prop_oneof![
        Just(NthTy::Child),
        Just(NthTy::LastChild),
        Just(NthTy::OnlyChild),
    ];
    let of = if depth == 0 {
        Just(None).boxed()
    } else {
        prop::option::weighted(0.15, prop::collection::vec(complex_strategy(0), 1..=1)).boxed()
    };
    (ty, -3i32..=3, -2i32..=5, of).prop_map(|(ty, a, b, of)| {
        // `of S` is only valid on :nth-child() / :nth-last-child().
        let of = match ty {
            NthTy::Child | NthTy::LastChild => of,
            NthTy::OnlyChild => None,
        };
        Simple::Nth { ty, a, b, of }
    })
}

fn simple_strategy(depth: u32) -> BoxedStrategy<Simple> {
    let leaf = prop_oneof![
        10 => prop::sample::select(PSEUDOS).prop_map(Simple::Pseudo),
        2 => prop::sample::select(CLASSES).prop_map(Simple::Class),
        1 => prop::sample::select(IDS).prop_map(Simple::Id),
        3 => prop::sample::select(ATTRS).prop_map(Simple::AttrExists),
        2 => prop::sample::select(TYPE_VALUES).prop_map(Simple::TypeEquals),
        3 => nth_strategy(depth),
        1 => Just(Simple::Root),
        1 => Just(Simple::Empty),
    ];
    if depth == 0 {
        return leaf.boxed();
    }
    prop_oneof![
        10 => leaf,
        2 => list_strategy(depth - 1).prop_map(Simple::Not),
        1 => list_strategy(depth - 1).prop_map(Simple::Is),
        // `:has()` may not nest, so its argument is generated at depth 0.
        3 => prop::collection::vec(
            (prop::option::of(comb_strategy()), complex_strategy(0)),
            1..=2,
        )
        .prop_map(Simple::Has),
    ]
    .boxed()
}

fn compound_strategy(depth: u32) -> BoxedStrategy<Compound> {
    // Two conditions at most: a compound that piles them up narrows its
    // answer to nothing, and a selector both implementations agree
    // matches nothing discriminates the least.
    (
        ty_strategy(),
        prop::collection::vec(simple_strategy(depth), 0..=2),
    )
        .prop_map(|(ty, simples)| Compound { ty, simples })
        .boxed()
}

fn complex_strategy(depth: u32) -> BoxedStrategy<Complex> {
    let steps = if depth == 0 { 0..=1 } else { 0..=2 };
    (
        compound_strategy(depth),
        prop::collection::vec((comb_strategy(), compound_strategy(depth)), steps),
    )
        .prop_map(|(head, tail)| Complex { head, tail })
        .boxed()
}

fn list_strategy(depth: u32) -> BoxedStrategy<Vec<Complex>> {
    prop::collection::vec(complex_strategy(depth), 1..=2).boxed()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// The property: for any selector the grammar can produce, the
    /// translated XPath selects exactly the elements Servo's matcher,
    /// driven by a reference reading of the HTML standard, says the
    /// selector matches.
    #[test]
    fn translation_selects_what_the_reference_matcher_selects(
        list in list_strategy(2),
    ) {
        let css = render_list(&list);
        // The recursion is bounded but not tightly, and `of S` doubles
        // the output per level; a rare giant selector says nothing the
        // small ones do not, and would only slow the run down.
        prop_assume!(css.len() <= 300);

        let (translated, reference) = ENV.with(|env| env.answers(&css));
        prop_assert_eq!(translated, reference, "{}", css);
    }
}

/// Report every mismatch in a sweep rather than aborting at the first,
/// naming the selector behind each one.
fn check(env: &Env, cases: impl IntoIterator<Item = String>) {
    let mut failures = String::new();
    let mut checked = 0;
    for css in cases {
        checked += 1;
        let (translated, reference) = env.answers(&css);
        if translated != reference {
            failures.push_str(&format!(
                "\n  {css:?}\n    translated {translated:?}\n    reference  {reference:?}"
            ));
        }
    }
    assert!(checked > 0, "the sweep generated no cases");
    assert!(failures.is_empty(), "differential mismatch:{failures}");
}

/// The fixture has to *discriminate*. A pseudo-class that matches
/// nothing in it — or everything — is pinned by the sweeps above only
/// in the sense that both implementations agree on an answer neither had
/// to work for, so this asserts that each one splits the document.
#[test]
fn every_pseudo_class_splits_the_fixture() {
    let env = Env::new();
    let all = env
        .fixture
        .evaluate("descendant-or-self::*", None)
        .expect("the fixture's elements");
    let mut counts = Vec::new();
    for pseudo in PSEUDOS {
        let (translated, reference) = env.answers(pseudo);
        assert_eq!(translated, reference, "{pseudo}");
        counts.push((pseudo, translated.len()));
    }
    let thin: Vec<_> = counts
        .iter()
        .filter(|(_, n)| *n < 2 || *n >= all.len())
        .collect();
    assert!(
        thin.is_empty(),
        "these pseudo-classes no longer split the {} elements of the fixture: {thin:?}",
        all.len()
    );
}

/// Every subject a compound can pin its local name with, or leave
/// unpinned, crossed with every pseudo-class.
///
/// This is the cross-product the property test reaches only by chance:
/// the translation folds each pseudo-class differently depending on
/// whether the compound pins a local name, and on whether that name is
/// one the pseudo-class applies to, so each of these is a separate arm
/// of `src/translate/pseudo.rs` rather than a separate input to one.
#[test]
fn every_pseudo_class_on_every_subject() {
    let env = Env::new();
    let mut subjects = vec![String::new(), "*".to_owned(), "xhtml|*".to_owned()];
    for name in TYPES {
        subjects.push(format!("*|{name}"));
        subjects.push(format!("xhtml|{name}"));
    }
    let cases = subjects.iter().flat_map(|subject| {
        PSEUDOS
            .iter()
            .map(move |pseudo| format!("{subject}{pseudo}"))
    });
    check(&env, cases);
}

/// Every pseudo-class in every position that changes how its condition
/// is placed: negated, inside the forgiving lists, as a `:has()`
/// argument under each combinator, as an `of S` argument, and on either
/// side of a combinator.
///
/// A pseudo-class translation is a condition on one step, and these are
/// the contexts that move that step: a wrong `or`-grouping or a
/// condition attached to the wrong step shows up here and nowhere else.
#[test]
fn every_pseudo_class_in_every_context() {
    let env = Env::new();
    let cases = PSEUDOS.iter().flat_map(|pseudo| {
        [
            format!(":not({pseudo})"),
            format!("xhtml|input:not({pseudo})"),
            format!(":is({pseudo}, xhtml|p)"),
            format!(":where(xhtml|div, {pseudo})"),
            format!("xhtml|form:has({pseudo})"),
            format!("xhtml|fieldset:has(> {pseudo})"),
            format!("*:has(+ {pseudo})"),
            format!("*:has(~ *|input{pseudo})"),
            format!("*|form *|input{pseudo}"),
            format!("*|form > *|input{pseudo}"),
            format!("*|input{pseudo} + *"),
            format!("* + *|input{pseudo}"),
            format!("*|input{pseudo} ~ *|input"),
            format!("*|body > *:nth-child(2n+1 of {pseudo})"),
            format!("*|input{pseudo}:nth-of-type(1)"),
            format!("{pseudo}{pseudo}"),
        ]
        .into_iter()
    });
    check(&env, cases);
}

/// The corners the fixture is arranged around, named one by one so a
/// failure says which rule broke instead of printing a shrunk blob.
#[test]
fn differential_regressions() {
    let env = Env::new();
    check(
        &env,
        [
            // The fieldset walk: the carve-out, the second legend, a
            // legend that is not a fieldset's child, and nesting.
            "*|legend *|input:enabled",
            "*|legend *|input:disabled",
            "*|fieldset[disabled] *|input:disabled",
            "*|div *|legend *|input:disabled",
            "*|fieldset:disabled",
            "*|fieldset:enabled",
            "*|button:disabled",
            "*|select:disabled",
            // The option/optgroup rules, which the fieldset rule must
            // not reach.
            "*|fieldset[disabled] *|option:enabled",
            "*|fieldset[disabled] *|optgroup:enabled",
            "*|optgroup[disabled] > *|option:disabled",
            "*|option:disabled",
            "*|optgroup:enabled",
            // `type` as an enumerated attribute, including the values
            // that are no keyword at all.
            "*|input:required",
            "*|input:optional",
            "*|input[type=\"\" s]:required",
            "*|input[type=\"wat\" s]:required",
            "*|input[type=\"hidden|range\" s]:required",
            "*|input[type=\"RANGE\" s]:required",
            "*|input[type=\"RANGE\" s]:optional",
            "*|select:required",
            "*|textarea:optional",
            // Mutability and editability.
            "*|input:read-write",
            "*|input:read-only",
            "*|textarea:read-write",
            "*|p:read-write",
            "*|div:read-write",
            "*|div[contenteditable] *|p:read-write",
            "*|div[contenteditable] *|p:read-only",
            ":root:read-only",
            // Default buttons, checkedness and placeholders.
            "*|button:default",
            "*|input:default",
            "*|option:default",
            "*|form:has(> *|button:default)",
            "*|input:checked",
            "*|option:checked",
            "*|input:placeholder-shown",
            "*|textarea:placeholder-shown",
            // Links.
            "*|a:link",
            "*|a:any-link",
            "*|area:link",
            "*|link:link",
            // Names outside every pseudo-class's element set, which
            // fold to "never matches".
            "*|p:disabled",
            "*|div:checked",
            "*|form:required",
            "*|p:default",
            "*|div:placeholder-shown",
            "*|p:link",
        ]
        .map(ToOwned::to_owned),
    );
}

/// The one divergence the generated selectors are steered around,
/// pinned so that it stays exactly this and nothing more.
///
/// HTML's pseudo-classes are defined over HTML *elements*, and the
/// reference matches them that way. The translation identifies elements
/// by `local-name()` alone, deliberately: the overrides are the one part
/// of a translation the caller cannot spell themselves, and a
/// qualified-name comparison would break for the `*|input` and
/// `h|input` subjects an XHTML document needs. The price is that an
/// element of the same local name in another namespace is treated as
/// the HTML one.
#[test]
fn html_pseudo_classes_match_any_namespace() {
    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
        <html xmlns="http://www.w3.org/1999/xhtml" xmlns:x="urn:example:x" id="doc">
          <input id="h1" disabled="disabled"/>
          <x:input id="x1" disabled="disabled"/>
        </html>"#;
    const NS: &[(&str, &str)] = &[("xhtml", XHTML_NS), ("x", "urn:example:x")];

    let env = Env::from_xml(XML, NS);
    let (translated, reference) = env.answers("*:disabled");
    assert_eq!(translated, ["h1", "x1"]);
    assert_eq!(reference, ["h1"]);
    // Everything else about the two agrees: the divergence is in which
    // elements the pseudo-class's own rules apply to, not in what the
    // rest of the compound matches.
    let (translated, reference) = env.answers("xhtml|input:disabled");
    assert_eq!(translated, reference);
    assert_eq!(translated, ["h1"]);
}
