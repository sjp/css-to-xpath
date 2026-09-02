//! Differential test: the translated XPath must select exactly the
//! elements Servo's own matcher selects.
//!
//! Every other execution test states its expectations by hand, so it can
//! only catch the mistakes someone thought to look for. Here the
//! expectation comes from a second, independent implementation: the
//! `selectors` crate this one already depends on ships a complete
//! matcher, and [`reference`] implements its `Element` trait over the
//! same document the XPath is evaluated against. A generated selector
//! then has two answers that must agree.
//!
//! # Why the reference re-implements `SelectorImpl`
//!
//! The crate's own `CssToXpathImpl` is `pub(crate)`, and its parser maps
//! a CSS prefix onto an XPath one rather than onto a namespace URI —
//! there is no URI for a matcher to compare against. The reference
//! therefore has its own `SelectorImpl` whose namespace URLs are real,
//! resolved from the fixture's prefix bindings. That makes it a genuinely
//! separate second source of truth rather than a mirror of the code under
//! test.
//!
//! # What the grammar does and does not generate
//!
//! Only `Mode::Generic`, and only shapes where the translation is exact:
//!
//! - **A named type selector is always written `|e`, never `e`.** An
//!   unprefixed type selector means the null namespace in this crate
//!   (see the README), while CSS — with no default namespace declared,
//!   which is the only thing a reference matcher can assume, since the
//!   translator has no namespace map to declare one from — reads `e` as
//!   "that name in any namespace". `|e` says the former outright, and
//!   the translator emits byte-identical output for the two, so nothing
//!   is left untested by writing it that way.
//! - **Of-type pseudo-classes only on a subject whose type is one
//!   `(namespace, local name)` pair.** They are unsupported on a wildcard
//!   subject, and on `*|e` the translation counts siblings by local name
//!   alone; [`any_namespace_of_type_counts_by_local_name`] pins that
//!   divergence rather than generating around it silently.
//! - No `:has()` inside `:has()`, which the translator rejects, and no
//!   pseudo-class whose answer is not in the document tree (`:hover`,
//!   `:checked`, `:lang()`), which the reference has no way to answer.

mod common;

use css_to_xpath::Mode;
use proptest::prelude::*;

use common::Fixture;
use reference::Reference;

/// The prefix bindings the fixture declares, shared by the XPath
/// evaluation context and the reference matcher's parser.
const NAMESPACES: &[(&str, &str)] = &[("x", "urn:example:x"), ("y", "urn:example:y")];

/// The fixture, and the reference matcher over the very same tree.
struct Env {
    fixture: Fixture,
    reference: Reference,
}

impl Env {
    fn new() -> Self {
        let fixture = Fixture::new(include_str!("fixtures/differential.xml"), NAMESPACES);
        let reference = Reference::new(fixture.document(), NAMESPACES);
        Env { fixture, reference }
    }

    /// The two answers for `css`: what the translated XPath selects, and
    /// what Servo's matcher selects.
    fn answers(&self, css: &str) -> (Vec<String>, Vec<String>) {
        let translated = self.fixture.select(css, Mode::Generic);
        let reference = self
            .reference
            .select(css)
            .unwrap_or_else(|e| panic!("the generated selector is not valid CSS: {e}"));
        (translated, reference)
    }
}

// Parsing the fixture and indexing it costs more than a single case, so
// both are done once. `Fixture` owns a non-`Sync` `Package`, hence a
// thread-local rather than a `OnceLock`.
thread_local! {
    static ENV: Env = Env::new();
}

/// The vocabulary the generated selectors draw on. Names, classes, ids
/// and values are mostly the fixture's own, so that generated selectors
/// match something more often than not; a few absent ones keep the
/// empty answer represented.
const TYPES: &[&str] = &["a", "b", "c", "café", "d"];
const PREFIXES: &[&str] = &["x", "y"];
const CLASSES: &[&str] = &["one", "two", "three", "four"];
const IDS: &[&str] = &["a1", "b3", "c2", "nope"];
const ATTRS: &[&str] = &["data-v", "role", "class", "title", "id", "absent"];
const VALUES: &[&str] = &[
    "alpha",
    "beta",
    "alpha-beta",
    "BETA",
    "gamma",
    "one",
    "a1",
    "",
];
const OPERATORS: &[&str] = &["=", "~=", "|=", "^=", "$=", "*="];

/// The type-selector part of a compound, which the grammar always
/// decides even when the answer is "no type selector at all".
#[derive(Clone, Debug)]
enum Ty {
    /// `*`
    Universal,
    /// `*|*`
    AnyNsUniversal,
    /// `|*`
    NoNsUniversal,
    /// `ns|*`
    NsUniversal(&'static str),
    /// No type selector at all: the compound is conditions only.
    Implicit,
    /// `*|e`
    AnyNsNamed(&'static str),
    /// `|e`
    NoNsNamed(&'static str),
    /// `ns|e`
    NsNamed(&'static str, &'static str),
}

impl Ty {
    fn render(&self) -> String {
        match *self {
            Ty::Universal => "*".to_owned(),
            Ty::AnyNsUniversal => "*|*".to_owned(),
            Ty::NoNsUniversal => "|*".to_owned(),
            Ty::NsUniversal(prefix) => format!("{prefix}|*"),
            Ty::Implicit => String::new(),
            Ty::AnyNsNamed(name) => format!("*|{name}"),
            Ty::NoNsNamed(name) => format!("|{name}"),
            Ty::NsNamed(prefix, name) => format!("{prefix}|{name}"),
        }
    }

    /// Whether this type names exactly one `(namespace, local name)`
    /// pair, which is what an of-type pseudo-class needs: the translator
    /// rejects the wildcard and implicit forms outright, and on `*|e` it
    /// counts by local name — see the module comment.
    fn is_single_type(&self) -> bool {
        matches!(*self, Ty::NoNsNamed(_) | Ty::NsNamed(..))
    }
}

/// The namespace part of an attribute selector.
#[derive(Clone, Debug)]
enum AttrNs {
    /// `[a]`
    None,
    /// `[*|a]`
    Any,
    /// `[ns|a]`
    Prefix(&'static str),
}

#[derive(Clone, Copy, Debug)]
enum NthTy {
    Child,
    LastChild,
    OfType,
    LastOfType,
    OnlyChild,
    OnlyOfType,
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
    Class(&'static str),
    Id(&'static str),
    Attr {
        ns: AttrNs,
        name: &'static str,
        /// `None` is the existence test; otherwise operator, value and
        /// whether the ASCII-case-insensitive flag is set.
        test: Option<(&'static str, &'static str, bool)>,
    },
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
    Where(Vec<Complex>),
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
    /// `single_type` is whether the compound's subject has one type, and
    /// so whether an of-type pseudo-class is translatable here; when it
    /// is not, the of-type form is rendered as its child counterpart
    /// rather than dropped, keeping the shape of the generated selector.
    fn render(&self, single_type: bool) -> String {
        match self {
            Simple::Class(name) => format!(".{name}"),
            Simple::Id(id) => format!("#{id}"),
            Simple::Attr { ns, name, test } => {
                let name = match ns {
                    AttrNs::None => (*name).to_owned(),
                    AttrNs::Any => format!("*|{name}"),
                    AttrNs::Prefix(prefix) => format!("{prefix}|{name}"),
                };
                match test {
                    None => format!("[{name}]"),
                    Some((operator, value, insensitive)) => {
                        let flag = if *insensitive { " i" } else { "" };
                        format!("[{name}{operator}\"{value}\"{flag}]")
                    }
                }
            }
            Simple::Nth { ty, a, b, of } => {
                let of = of
                    .as_ref()
                    .map_or_else(String::new, |list| format!(" of {}", render_list(list)));
                let an_plus_b = render_an_plus_b(*a, *b);
                match (ty, single_type) {
                    (NthTy::Child, _) => format!(":nth-child({an_plus_b}{of})"),
                    (NthTy::LastChild, _) => format!(":nth-last-child({an_plus_b}{of})"),
                    (NthTy::OnlyChild, _) => ":only-child".to_owned(),
                    (NthTy::OfType, true) => format!(":nth-of-type({an_plus_b})"),
                    (NthTy::LastOfType, true) => format!(":nth-last-of-type({an_plus_b})"),
                    (NthTy::OnlyOfType, true) => ":only-of-type".to_owned(),
                    (NthTy::OfType, false) => format!(":nth-child({an_plus_b})"),
                    (NthTy::LastOfType, false) => format!(":nth-last-child({an_plus_b})"),
                    (NthTy::OnlyOfType, false) => ":only-child".to_owned(),
                }
            }
            Simple::Root => ":root".to_owned(),
            Simple::Empty => ":empty".to_owned(),
            Simple::Not(list) => format!(":not({})", render_list(list)),
            Simple::Is(list) => format!(":is({})", render_list(list)),
            Simple::Where(list) => format!(":where({})", render_list(list)),
            Simple::Has(relatives) => {
                let args: Vec<String> = relatives
                    .iter()
                    .map(|(combinator, complex)| {
                        let lead = match combinator {
                            None => "",
                            Some(Comb::Descendant) => "",
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
    let single_type = compound.ty.is_single_type();
    // A compound of nothing at all is not a selector; `*` is what an
    // implicit type means anyway.
    if matches!(compound.ty, Ty::Implicit) && compound.simples.is_empty() {
        return "*".to_owned();
    }
    let mut out = compound.ty.render();
    for simple in &compound.simples {
        out.push_str(&simple.render(single_type));
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
    let prefix = || prop::sample::select(PREFIXES);
    prop_oneof![
        2 => Just(Ty::Universal),
        1 => Just(Ty::AnyNsUniversal),
        1 => Just(Ty::NoNsUniversal),
        1 => prefix().prop_map(Ty::NsUniversal),
        6 => Just(Ty::Implicit),
        2 => name().prop_map(Ty::AnyNsNamed),
        2 => name().prop_map(Ty::NoNsNamed),
        3 => (prefix(), name()).prop_map(|(p, n)| Ty::NsNamed(p, n)),
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

fn attr_strategy() -> impl Strategy<Value = Simple> {
    let ns = prop_oneof![
        4 => Just(AttrNs::None),
        1 => Just(AttrNs::Any),
        1 => prop::sample::select(PREFIXES).prop_map(AttrNs::Prefix),
    ];
    let test = prop::option::of((
        prop::sample::select(OPERATORS),
        prop::sample::select(VALUES),
        any::<bool>(),
    ));
    (ns, prop::sample::select(ATTRS), test).prop_map(|(ns, name, test)| Simple::Attr {
        ns,
        name,
        test,
    })
}

/// `of S` doubles the translated output per nesting level, so its
/// argument is always generated at depth 0.
fn nth_strategy(depth: u32) -> impl Strategy<Value = Simple> {
    let ty = prop_oneof![
        Just(NthTy::Child),
        Just(NthTy::LastChild),
        Just(NthTy::OfType),
        Just(NthTy::LastOfType),
        Just(NthTy::OnlyChild),
        Just(NthTy::OnlyOfType),
    ];
    let of = if depth == 0 {
        Just(None).boxed()
    } else {
        prop::option::weighted(0.3, prop::collection::vec(complex_strategy(0), 1..=2)).boxed()
    };
    (ty, -4i32..=4, -4i32..=8, of).prop_map(|(ty, a, b, of)| {
        // `of S` is only valid on :nth-child() / :nth-last-child().
        let of = match ty {
            NthTy::Child | NthTy::LastChild => of,
            _ => None,
        };
        Simple::Nth { ty, a, b, of }
    })
}

fn simple_strategy(depth: u32) -> BoxedStrategy<Simple> {
    let leaf = prop_oneof![
        4 => prop::sample::select(CLASSES).prop_map(Simple::Class),
        2 => prop::sample::select(IDS).prop_map(Simple::Id),
        4 => attr_strategy(),
        5 => nth_strategy(depth),
        1 => Just(Simple::Root),
        2 => Just(Simple::Empty),
    ];
    if depth == 0 {
        return leaf.boxed();
    }
    prop_oneof![
        10 => leaf,
        2 => list_strategy(depth - 1).prop_map(Simple::Not),
        1 => list_strategy(depth - 1).prop_map(Simple::Is),
        1 => list_strategy(depth - 1).prop_map(Simple::Where),
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
    #![proptest_config(ProptestConfig { cases: 1024, ..ProptestConfig::default() })]

    /// The property: for any selector the grammar can produce, the
    /// translated XPath selects exactly the elements Servo's matcher
    /// says the selector matches.
    #[test]
    fn translation_selects_what_the_reference_matcher_selects(
        list in list_strategy(2),
    ) {
        let css = render_list(&list);
        // The recursion is bounded but not tightly, and `of S` doubles
        // the output per level; a rare giant selector says nothing the
        // small ones do not, and would only slow the run down.
        prop_assume!(css.len() <= 400);

        let (translated, reference) = ENV.with(|env| env.answers(&css));
        prop_assert_eq!(translated, reference, "{}", css);
    }
}

/// Attribute selectors are a small finite cross-product — name,
/// namespace, operator, value, case flag — so they are exhausted rather
/// than sampled. Random selectors reach any one combination too rarely:
/// a translation of `[a|=v]` that dropped the equality half of the
/// dash-match survived a 512-case run of the property test below.
#[test]
fn every_attribute_selector_shape() {
    let env = Env::new();
    let mut failures = String::new();
    let mut checked = 0;
    for prefix in ["", "*|", "x|", "y|"] {
        for name in ATTRS {
            let mut cases = vec![format!("[{prefix}{name}]")];
            for operator in OPERATORS {
                for value in VALUES {
                    for flag in ["", " i", " s"] {
                        cases.push(format!("[{prefix}{name}{operator}\"{value}\"{flag}]"));
                    }
                }
            }
            for css in cases {
                checked += 1;
                let (translated, reference) = env.answers(&css);
                if translated != reference {
                    failures.push_str(&format!(
                        "\n  {css:?}\n    translated {translated:?}\n    reference  {reference:?}"
                    ));
                }
            }
        }
    }
    assert!(
        checked > 1000,
        "the cross-product shrank to {checked} cases"
    );
    assert!(failures.is_empty(), "differential mismatch:{failures}");
}

/// The of-type pseudo-classes across every subject the translator
/// accepts them on, for the same reason: the sibling node test they
/// count with depends on the subject's namespace and on whether its name
/// can be written as a node test at all.
#[test]
fn every_of_type_shape() {
    let env = Env::new();
    let mut failures = String::new();
    for subject in ["|a", "|b", "|c", "|café", "x|b", "x|c", "x|café", "y|b"] {
        let mut cases = vec![
            format!("{subject}:first-of-type"),
            format!("{subject}:last-of-type"),
            format!("{subject}:only-of-type"),
        ];
        for a in [-2, -1, 0, 1, 2, 3] {
            for b in [-1, 0, 1, 2, 3] {
                cases.push(format!("{subject}:nth-of-type({})", render_an_plus_b(a, b)));
                cases.push(format!(
                    "{subject}:nth-last-of-type({})",
                    render_an_plus_b(a, b)
                ));
            }
        }
        for css in cases {
            let (translated, reference) = env.answers(&css);
            if translated != reference {
                failures.push_str(&format!(
                    "\n  {css:?}\n    translated {translated:?}\n    reference  {reference:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "differential mismatch:{failures}");
}

/// Interactions worth pinning deterministically: each one exercises a
/// path the grammar reaches only rarely, and a failure here names the
/// construct instead of a shrunk blob.
#[test]
fn differential_regressions() {
    let cases = [
        // :has() inside :not() inside `An+B of S`.
        "|b:nth-child(2n+1 of :not(:has(+ |c)))",
        "|a:nth-last-child(-n+2 of |b:has(> |c), |c)",
        // A name XPath cannot write as a node test, under every
        // combinator and either side of one.
        "|café + |b",
        "|b + |café",
        "x|café ~ |b",
        "|c > |café:only-of-type",
        "|café:nth-of-type(1)",
        // A namespaced subject under the of-type pseudos.
        "x|b:nth-last-of-type(1)",
        "x|b:only-of-type",
        "|b:nth-of-type(2n)",
        // The + combinator's position predicate, with a prefixed and a
        // wildcard node test on the right.
        "|b + x|b",
        "|b + *|b",
        "|b + *",
        // :has() with each leading combinator, and multi-step arguments.
        "|a:has(> |b.one)",
        "|a:has(+ |a)",
        "|a:has(~ |café)",
        "|a:has(|c |b)",
        "|a:has(|b + |c)",
        "|a:has(> |c > |b)",
        "|c:has(|b ~ |b)",
        // Nesting of the forgiving-list pseudos.
        "|b:is(:not(.one), :where(.two))",
        "*:not(:is(|a, |b, |c))",
        // Attribute forms whose translation is not a simple comparison.
        "[data-v=\"\"]",
        "[data-v^=\"\"]",
        "[data-v~=\"alpha beta\"]",
        "[*|role]",
        "[x|role=\"lead\"]",
        "[data-v=\"beta\" i]",
        "[title=\"\"]",
        // The structural pseudos on the root element.
        ":root",
        ":root:only-child",
        ":empty",
        ":not(:empty)",
    ];

    let env = Env::new();
    let mut failures = String::new();
    for css in cases {
        let (translated, reference) = env.answers(css);
        if translated != reference {
            failures.push_str(&format!(
                "\n  {css:?}\n    translated {translated:?}\n    reference  {reference:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "differential mismatch:{failures}");
}

/// The one divergence the generated selectors are steered around, pinned
/// so that it stays exactly this and nothing more.
///
/// `*|b` is `*[local-name() = 'b']`, and the of-type pseudos count
/// siblings by that same node test — by local name alone. Per the spec
/// `b` and `x:b` are different types, so Servo counts them separately
/// and calls `x:b` the first of its own type.
#[test]
fn any_namespace_of_type_counts_by_local_name() {
    let env = Env::new();
    let (translated, reference) = env.answers("*|b:first-of-type");
    assert_eq!(translated, ["b1", "b6", "b7"]);
    assert_eq!(reference, ["b1", "b3", "b6", "b7"]);
    // Everything else about `*|b` agrees: the divergence is in the
    // counting, not in what the type selector itself matches.
    let (translated, reference) = env.answers("*|b");
    assert_eq!(translated, reference);
}

/// The reference matcher: Servo's `selectors` matching engine driven
/// over the fixture tree.
///
/// The tree is copied into a flat arena in document order rather than
/// matched over `sxd_document`'s handles directly. Two reasons: an
/// `Element` must hand out a stable `OpaqueElement` for identity
/// comparisons (`:has()`'s anchor, the nth-index cache), and sxd's
/// element handles are `Copy` values with no publicly reachable address;
/// and the sibling/child navigation the trait wants is a plain index
/// lookup here rather than a scan that skips non-element nodes.
mod reference {
    use std::borrow::Borrow;
    use std::fmt;

    use cssparser::{Parser as CssParser, ParserInput, ToCss};
    use precomputed_hash::PrecomputedHash;
    use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
    use selectors::bloom::BloomFilter;
    use selectors::context::{
        MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
        SelectorCaches,
    };
    use selectors::matching::{ElementSelectorFlags, matches_selector};
    use selectors::parser::{
        NonTSPseudoClass, ParseRelative, PseudoElement, SelectorImpl, SelectorList,
        SelectorParseErrorKind,
    };
    use selectors::{Element, OpaqueElement};
    use sxd_document::dom::{ChildOfElement, ChildOfRoot, Document, Element as SxdElement};

    /// CSS white space, which is what a `class` attribute and `[a~=v]`
    /// split on. (Note the form feed, which XPath's `normalize-space`
    /// does not treat as white space — a divergence the README records.)
    const CSS_WHITESPACE: [char; 5] = [' ', '\t', '\r', '\n', '\u{c}'];

    /// A string-ish associated type: identifiers, local names,
    /// attribute values and namespace prefixes.
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub(crate) struct Name(String);

    impl<'a> From<&'a str> for Name {
        fn from(s: &'a str) -> Self {
            Name(s.to_owned())
        }
    }

    impl AsRef<str> for Name {
        fn as_ref(&self) -> &str {
            &self.0
        }
    }

    impl Borrow<str> for Name {
        fn borrow(&self) -> &str {
            &self.0
        }
    }

    impl ToCss for Name {
        fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
            cssparser::serialize_identifier(&self.0, dest)
        }
    }

    impl PrecomputedHash for Name {
        fn precomputed_hash(&self) -> u32 {
            // Only the bloom filters use this, and matching here runs
            // without one; a constant is consistent with Eq.
            0
        }
    }

    /// A namespace URL. Unlike the translator's, this one is a real URL,
    /// resolved from the fixture's prefix bindings, so that matching can
    /// compare it against a document node's namespace. The empty URL is
    /// "no namespace", which is also the declared default namespace —
    /// see the module comment on unprefixed type selectors.
    #[derive(Clone, Debug, Default, Eq, PartialEq)]
    pub(crate) struct NsUrl(String);

    impl<'a> From<&'a str> for NsUrl {
        fn from(s: &'a str) -> Self {
            NsUrl(s.to_owned())
        }
    }

    impl Borrow<str> for NsUrl {
        fn borrow(&self) -> &str {
            &self.0
        }
    }

    impl ToCss for NsUrl {
        fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
            cssparser::serialize_string(&self.0, dest)
        }
    }

    impl PrecomputedHash for NsUrl {
        fn precomputed_hash(&self) -> u32 {
            0
        }
    }

    /// The pseudo-class and pseudo-element types. The reference answers
    /// nothing that is not in the document tree, and the parser below
    /// keeps the trait's rejecting defaults for every such name, so no
    /// value of this type is ever constructed. (An uninhabited type
    /// would say that outright, but a reference to one cannot be matched
    /// without tripping `uninhabited_references`.)
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) struct Never;

    impl Never {
        fn unreachable() -> ! {
            unreachable!("the reference parser accepts no pseudo-class or pseudo-element")
        }
    }

    impl ToCss for Never {
        fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
            Never::unreachable()
        }
    }

    impl NonTSPseudoClass for Never {
        type Impl = RefImpl;

        fn is_active_or_hover(&self) -> bool {
            Never::unreachable()
        }

        fn is_user_action_state(&self) -> bool {
            Never::unreachable()
        }
    }

    impl PseudoElement for Never {
        type Impl = RefImpl;
    }

    #[derive(Clone, Debug)]
    pub(crate) struct RefImpl;

    impl SelectorImpl for RefImpl {
        type ExtraMatchingData<'a> = ();
        type AttrValue = Name;
        type Identifier = Name;
        type LocalName = Name;
        type NamespaceUrl = NsUrl;
        type NamespacePrefix = Name;
        type BorrowedNamespaceUrl = str;
        type BorrowedLocalName = str;
        type NonTSPseudoClass = Never;
        type PseudoElement = Never;
    }

    /// The selector parser. Only the hooks that widen the grammar are
    /// overridden; everything else — pseudo-elements, `:hover` and the
    /// rest of the non-tree-structural set — keeps the trait's default,
    /// which is to reject.
    struct RefParser {
        /// Prefix to namespace URL, as the fixture declares them.
        namespaces: Vec<(String, String)>,
    }

    impl<'i> selectors::parser::Parser<'i> for RefParser {
        type Impl = RefImpl;
        type Error = SelectorParseErrorKind<'i>;

        fn parse_is_and_where(&self) -> bool {
            true
        }

        fn is_is_alias(&self, name: &str) -> bool {
            name.eq_ignore_ascii_case("matches")
        }

        fn parse_has(&self) -> bool {
            true
        }

        fn parse_nth_child_of(&self) -> bool {
            true
        }

        fn namespace_for_prefix(&self, prefix: &Name) -> Option<NsUrl> {
            self.namespaces
                .iter()
                .find(|(p, _)| *p == prefix.0)
                .map(|(_, url)| NsUrl(url.clone()))
        }

        /// No default namespace, which is CSS's own reading of a
        /// stylesheet that declares none: an unprefixed type selector
        /// matches its name in any namespace, and so does `*`. The
        /// translator instead reads an unprefixed type selector as the
        /// null namespace, which is why the grammar writes `|e` rather
        /// than `e` — see the module comment.
        fn default_namespace(&self) -> Option<NsUrl> {
            None
        }
    }

    struct Attr {
        namespace: String,
        local: String,
        value: String,
    }

    /// One element, flattened out of the document.
    struct Node {
        /// How the node is reported, matching the XPath harness: its id.
        label: String,
        namespace: String,
        local: String,
        id: Option<String>,
        classes: Vec<String>,
        attributes: Vec<Attr>,
        parent: Option<usize>,
        /// Element children only, in document order.
        children: Vec<usize>,
        /// This node's index within its parent's `children`.
        sibling_index: usize,
        empty: bool,
    }

    /// The elements of one document, in document order.
    struct Dom {
        nodes: Vec<Node>,
    }

    impl Dom {
        fn build(document: Document<'_>) -> Self {
            let mut dom = Dom { nodes: Vec::new() };
            for child in document.root().children() {
                if let ChildOfRoot::Element(element) = child {
                    dom.push(element, None);
                }
            }
            dom
        }

        fn push(&mut self, element: SxdElement<'_>, parent: Option<usize>) {
            let index = self.nodes.len();
            let name = element.name();
            let attributes: Vec<Attr> = element
                .attributes()
                .into_iter()
                .map(|attribute| Attr {
                    namespace: attribute.name().namespace_uri().unwrap_or("").to_owned(),
                    local: attribute.name().local_part().to_owned(),
                    value: attribute.value().to_owned(),
                })
                .collect();
            let id = element.attribute_value("id").map(ToOwned::to_owned);
            let classes = element.attribute_value("class").map_or_else(Vec::new, |c| {
                c.split(CSS_WHITESPACE)
                    .filter(|token| !token.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            });
            self.nodes.push(Node {
                label: id
                    .clone()
                    .unwrap_or_else(|| format!("<{} with no id>", name.local_part())),
                namespace: name.namespace_uri().unwrap_or("").to_owned(),
                local: name.local_part().to_owned(),
                id,
                classes,
                attributes,
                parent,
                children: Vec::new(),
                sibling_index: parent.map_or(0, |p| self.nodes[p].children.len()),
                // Selectors Level 3: no element children and no
                // non-empty text, which is the translation's reading too.
                empty: element.children().into_iter().all(|child| match child {
                    ChildOfElement::Element(_) => false,
                    ChildOfElement::Text(text) => text.text().is_empty(),
                    _ => true,
                }),
            });
            if let Some(parent) = parent {
                self.nodes[parent].children.push(index);
            }
            for child in element.children() {
                if let ChildOfElement::Element(child) = child {
                    self.push(child, Some(index));
                }
            }
        }
    }

    /// A handle onto one element of a [`Dom`].
    #[derive(Clone, Copy)]
    struct El<'a> {
        dom: &'a Dom,
        index: usize,
    }

    impl fmt::Debug for El<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "<{} id={}>", self.node().local, self.node().label)
        }
    }

    impl<'a> El<'a> {
        fn node(&self) -> &'a Node {
            &self.dom.nodes[self.index]
        }

        fn at(&self, index: usize) -> El<'a> {
            El {
                dom: self.dom,
                index,
            }
        }

        fn sibling(&self, offset: isize) -> Option<El<'a>> {
            let node = self.node();
            let parent = &self.dom.nodes[node.parent?];
            let index = node.sibling_index.checked_add_signed(offset)?;
            parent.children.get(index).map(|&index| self.at(index))
        }
    }

    impl Element for El<'_> {
        type Impl = RefImpl;

        fn opaque(&self) -> OpaqueElement {
            // The arena is never mutated after it is built, so a node's
            // address is a stable identity for as long as matching runs.
            OpaqueElement::new(self.node())
        }

        fn parent_element(&self) -> Option<Self> {
            self.node().parent.map(|index| self.at(index))
        }

        fn parent_node_is_shadow_root(&self) -> bool {
            false
        }

        fn containing_shadow_host(&self) -> Option<Self> {
            None
        }

        fn is_pseudo_element(&self) -> bool {
            false
        }

        fn prev_sibling_element(&self) -> Option<Self> {
            self.sibling(-1)
        }

        fn next_sibling_element(&self) -> Option<Self> {
            self.sibling(1)
        }

        fn first_element_child(&self) -> Option<Self> {
            self.node().children.first().map(|&index| self.at(index))
        }

        fn is_html_element_in_html_document(&self) -> bool {
            false
        }

        fn has_local_name(&self, local_name: &str) -> bool {
            self.node().local == local_name
        }

        fn has_namespace(&self, ns: &str) -> bool {
            self.node().namespace == ns
        }

        fn is_same_type(&self, other: &Self) -> bool {
            let (this, other) = (self.node(), other.node());
            this.local == other.local && this.namespace == other.namespace
        }

        fn attr_matches(
            &self,
            ns: &NamespaceConstraint<&NsUrl>,
            local_name: &Name,
            operation: &AttrSelectorOperation<&Name>,
        ) -> bool {
            self.node()
                .attributes
                .iter()
                .filter(|attribute| {
                    attribute.local == local_name.0
                        && match ns {
                            NamespaceConstraint::Any => true,
                            NamespaceConstraint::Specific(url) => attribute.namespace == url.0,
                        }
                })
                .any(|attribute| operation.eval_str(&attribute.value))
        }

        fn match_non_ts_pseudo_class(
            &self,
            _pc: &Never,
            _context: &mut MatchingContext<RefImpl>,
        ) -> bool {
            Never::unreachable()
        }

        fn match_pseudo_element(
            &self,
            _pe: &Never,
            _context: &mut MatchingContext<RefImpl>,
        ) -> bool {
            Never::unreachable()
        }

        fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {
            // Matching runs with NeedsSelectorFlags::No.
        }

        fn is_link(&self) -> bool {
            false
        }

        fn is_html_slot_element(&self) -> bool {
            false
        }

        fn has_id(&self, id: &Name, case_sensitivity: CaseSensitivity) -> bool {
            self.node()
                .id
                .as_ref()
                .is_some_and(|own| case_sensitivity.eq(own.as_bytes(), id.0.as_bytes()))
        }

        fn has_class(&self, name: &Name, case_sensitivity: CaseSensitivity) -> bool {
            self.node()
                .classes
                .iter()
                .any(|own| case_sensitivity.eq(own.as_bytes(), name.0.as_bytes()))
        }

        fn has_custom_state(&self, _name: &Name) -> bool {
            false
        }

        fn imported_part(&self, _name: &Name) -> Option<Name> {
            None
        }

        fn is_part(&self, _name: &Name) -> bool {
            false
        }

        fn is_empty(&self) -> bool {
            self.node().empty
        }

        fn is_root(&self) -> bool {
            self.node().parent.is_none()
        }

        fn add_element_unique_hashes(&self, _filter: &mut BloomFilter) -> bool {
            false
        }
    }

    /// A document indexed for matching, plus the parser that resolves
    /// its namespace prefixes.
    pub(crate) struct Reference {
        dom: Dom,
        parser: RefParser,
    }

    impl Reference {
        pub(crate) fn new(document: Document<'_>, namespaces: &[(&str, &str)]) -> Self {
            Reference {
                dom: Dom::build(document),
                parser: RefParser {
                    namespaces: namespaces
                        .iter()
                        .map(|(prefix, url)| ((*prefix).to_owned(), (*url).to_owned()))
                        .collect(),
                },
            }
        }

        /// The labels of the elements `css` matches, in document order,
        /// or the parse error if the reference parser rejects it.
        pub(crate) fn select(&self, css: &str) -> Result<Vec<String>, String> {
            let mut input = ParserInput::new(css);
            let mut parser = CssParser::new(&mut input);
            let list = SelectorList::parse(&self.parser, &mut parser, ParseRelative::No)
                .map_err(|e| format!("{css:?}: {e:?}"))?;

            let mut selected = Vec::new();
            for index in 0..self.dom.nodes.len() {
                let element = El {
                    dom: &self.dom,
                    index,
                };
                // A fresh context per element: the caches are an
                // optimisation this reference has no need of, and a
                // reference that shares no state between elements is
                // one less thing to trust.
                let mut caches = SelectorCaches::default();
                let mut context = MatchingContext::new(
                    MatchingMode::Normal,
                    None,
                    &mut caches,
                    QuirksMode::NoQuirks,
                    NeedsSelectorFlags::No,
                    MatchingForInvalidation::No,
                );
                if list
                    .slice()
                    .iter()
                    .any(|selector| matches_selector(selector, 0, None, &element, &mut context))
                {
                    selected.push(self.dom.nodes[index].label.clone());
                }
            }
            Ok(selected)
        }
    }
}
