# css-to-xpath

[![Crates.io](https://img.shields.io/crates/v/css-to-xpath.svg)](https://crates.io/crates/css-to-xpath)
[![Docs.rs](https://docs.rs/css-to-xpath/badge.svg)](https://docs.rs/css-to-xpath)
[![CI](https://github.com/sjp/css-to-xpath/actions/workflows/ci.yml/badge.svg)](https://github.com/sjp/css-to-xpath/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Translate CSS selectors to XPath 1.0 expressions.

`css-to-xpath` parses CSS with [Servo's](https://github.com/servo/servo)
own `selectors` and `cssparser` crates. It uses this to construct XPath 1.0 expressions so that they can be evaluated using XML libraries such as `libxml2`.

## Installation

```sh
cargo add css-to-xpath
```

```toml
[dependencies]
css-to-xpath = "0.2"
```

## Quick start

```rust,no_run
use css_to_xpath::{css_to_xpath, Mode};

// mode: Mode::Generic | Mode::Html | Mode::Xhtml; prefix: prepended to the result.
assert_eq!(
    css_to_xpath("div.warning > a", "", Mode::Generic).unwrap(),
    "div[@class and contains(concat(' ', normalize-space(@class), ' '), ' warning ')]/a"
);

assert_eq!(
    css_to_xpath("li:nth-child(odd)", "", Mode::Generic).unwrap(),
    "li[count(preceding-sibling::*) mod 2 = 0]"
);
```

For repeated translations, build a `Translator` once and reuse it:

```rust,no_run
use css_to_xpath::{Mode, Translator};

let translator = Translator::new(Mode::Generic);
let xpath = translator.css_to_xpath("e:has(> .foo)", "").unwrap();
assert_eq!(
    xpath,
    "e[child::*[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]"
);
```

## Translator flavours

`Translator::new` takes one of three `Mode` variants:

- **`Mode::Generic`** — plain CSS/XPath semantics, case-sensitive names, no
  HTML-specific pseudo-classes.
- **`Mode::Html`** — lowercases element and attribute names (as HTML parsing
  does) and gives dynamic-seeming pseudo-classes their static HTML
  meaning where one exists. Each is limited to the elements HTML
  defines it over, so nothing else matches:
  - `:link`/`:any-link` — an `a` or `area` with an `href`.
  - `:checked` — a checked `input` of type `checkbox` or `radio`, or a
    selected `option`.
  - `:disabled`/`:enabled` — the two halves of HTML's "actually
    disabled" over `button`, `input`, `select`, `textarea`, `optgroup`,
    `option` and `fieldset`: the `disabled` attribute, an `option`
    under a disabled `optgroup`, and a disabled `fieldset` ancestor —
    with the carve-out that the fieldset's first `legend` keeps its
    contents enabled. The two partition that element set. (HTML also
    lists form-associated custom elements, which no static translation
    can recognise.)
  - `:required`/`:optional` — the `required` attribute over `select`,
    `textarea` and the `input` types it applies to.
  - `:lang()` — nearest `@lang` ancestor, case-folded prefix match.
- **`Mode::Xhtml`** — the same HTML pseudo-class semantics as `Mode::Html`,
  but preserves case (XHTML is XML, so names are case-sensitive) and
  reads `xml:lang` as well as `lang` for `:lang()`, preferring `xml:lang`
  when both are on the nearest ancestor (HTML's language determination).
  The element names *inside* these pseudo-classes — the `fieldset`
  ancestor, the parent `optgroup`, the `a`/`area` of `:link` — are matched
  by local name, so they see XHTML's namespaced elements and work with
  `*|input` and `h|input` subjects alike. Only the names you write follow
  the namespace rule below.

Pseudo-classes with no static equivalent (`:hover`, `:visited`,
`:focus`, …) always translate to an unmatchable `[0]` rather than
erroring, in every flavour.

## The `prefix` argument

`prefix` is prepended to each translated selector-group branch — pass
`"descendant-or-self::"` to search an entire subtree, or `""` for a bare
expression:

```rust,no_run
use css_to_xpath::{css_to_xpath, Mode};

assert_eq!(
    css_to_xpath("a, b", "descendant-or-self::", Mode::Generic).unwrap(),
    "descendant-or-self::a | descendant-or-self::b"
);
```

A selector group anchored on `:scope` ignores `prefix` and instead
anchors on the `self::` axis, since `:scope` names the context node the
XPath is evaluated from:

```rust,no_run
use css_to_xpath::{css_to_xpath, Mode};

assert_eq!(
    css_to_xpath(":scope > a", "descendant-or-self::", Mode::Generic).unwrap(),
    "self::*/a"
);
```

## Supported selectors

- Type, universal (`*`), and namespace selectors (`ns|e`, `*|e`, `|e`).
- ID (`#id`) and class (`.class`) selectors.
- Attribute selectors — `[attr]`, `=`, `~=`, `|=`, `^=`, `$=`, `*=` —
  with the Level 4 `i`/`s` case-sensitivity flags.
- Combinators: descendant (` `), child (`>`), next-sibling (`+`), and
  subsequent-sibling (`~`), including selector lists (`a, b`).
- The full nth-family: `:nth-child()`, `:nth-last-child()`,
  `:nth-of-type()`, `:nth-last-of-type()`, `:first-child`,
  `:last-child`, `:first-of-type`, `:last-of-type`, `:only-child`,
  `:only-of-type`, and the Level 4 `An+B of S` syntax.
- `:is()` / `:matches()` (legacy alias) / `:where()` / `:not()` /
  `:has()`, including complex (combinator-bearing) arguments and
  relative-selector leading combinators inside `:has()`. An empty
  `:is()` / `:where()` argument list is valid and matches nothing
  (`:is()` translates to `*[0]`), as the forgiving-selector-list grammar
  requires; the rest of forgiveness is not adopted, so an argument that
  fails to parse is an error rather than a silently dropped one.
- `:scope`, `:root`, `:empty`, `:lang()`. Under `Mode::Generic` a range
  translates to XPath's `lang()`, except the wildcard `:lang(*)` —
  "any known language", which `lang()` cannot express — which walks
  `@xml:lang` instead. `Mode::Xhtml` reads `@xml:lang` for every range.
  Both rely on the `xml` prefix, which XML binds implicitly and so needs
  no entry in the caller's namespace map; processors that do not pre-bind
  it need it registered.
- The `Mode::Html`/`Mode::Xhtml` form and link pseudo-classes listed above.

## Namespaces

A CSS namespace prefix is passed straight through to the XPath, so
`svg|g` becomes `svg:g` and the *caller's* namespace map decides what
`svg` binds to — this crate never sees namespace URLs, and a prefix that
is not a valid XPath name is an error rather than a guess.

An *unprefixed* type name becomes an unprefixed XPath name test, which
matches the null namespace only. That is the rule everywhere the name can
appear — at the top level, on the right of a combinator, and inside
`:is()`, `:where()`, `:not()`, `:has()` and `An+B of S`, where it becomes
the equivalent `self::` test:

```rust,no_run
use css_to_xpath::{css_to_xpath, Mode};

assert_eq!(css_to_xpath("body > p", "", Mode::Generic).unwrap(), "body/p");
assert_eq!(
    css_to_xpath(":is(body > p)", "", Mode::Generic).unwrap(),
    "*[self::p and parent::*[self::body]]"
);
```

So in a document with a *default* namespace — XHTML, SVG, Atom, … — a
bare `p` matches nothing, exactly as it would in an XPath expression
written by hand. Ask for the name in any namespace with `*|e`, which
translates to a `local-name()` test and is likewise the same wherever it
is written:

```rust,no_run
use css_to_xpath::{css_to_xpath, Mode};

assert_eq!(
    css_to_xpath("*|body > *|p", "", Mode::Generic).unwrap(),
    "*[local-name() = 'body']/*[local-name() = 'p']"
);
```

The other forms follow from the same rule: `|e` is "no namespace", which
is what a bare `e` already means, and a name needing quoting cannot be a
node test at all, so it folds into `name() = '…' and namespace-uri() = ''`
— the qualified-name comparison alone would also match the name in a
default namespace. Attribute names work the same way, except that an
unprefixed one has no namespace by definition, so `[foo]` and `[|foo]`
are the same test and `[*|foo]` is the any-namespace one.

## Not supported

These error rather than approximate, since XPath 1.0 has no way to
express them faithfully:

- Pseudo-elements (`::before`, `::slotted()`, `::part()`).
- The Level 4 column combinator (`||`) and `:nth-col()`/`:nth-last-col()`.
- Non-standard extensions: `[attr!=value]`, `:contains()`.
- `:dir()` (needs resolved bidi directionality) and other pseudo-classes
  outside the never-match allow-list, such as `:valid`, `:read-only`,
  and `:placeholder-shown` — these error instead of silently matching
  nothing, so typos stay loud.
- of-type pseudos (`:first-of-type`, `:nth-of-type()`, …) on any
  wildcard subject (`*`, `*|*`, `|*`, `ns|*`) or implicit-type compound:
  XPath 1.0 cannot compare a sibling's name against the matched
  element's own.
- Nested `:has()`, `:host`, and the `&` parent selector.
- Namespace prefixes that need quoting (`\31 ns|div`): a prefix that is
  not a valid XPath name cannot appear in a node test, and XPath 1.0
  cannot resolve one without the namespace URI, which this crate never
  sees. A *local name* needing quoting is fine — `svg|di\[v` translates to
  `svg:*[local-name() = 'di[v']`, so the prefix still resolves through the
  caller's namespace map.
- `:scope` outside the leftmost compound, or inside a functional
  pseudo-class argument.
- Functional pseudo-classes (`:is()`, `:not()`, `:where()`, `:has()`,
  `:nth-child(… of S)`) nested more than **64** levels deep. Parsing and
  translating both recurse once per level, so the depth is capped to turn
  a pathological selector into an error instead of a stack overflow.
  Nothing hand-written comes close; only the nesting depth is limited,
  not the length of a selector or of an argument chain.
- `:nth-child(… of S)` / `:nth-last-child(… of S)` nested more than **8**
  levels deep, or a single `of S` list translating to more than **1 MiB**.
  XPath 1.0 has no variables, so `S` has to be written out twice — once to
  filter the siblings being counted, once to constrain the element being
  matched — and a nested `of S` lands in both copies, so the output
  doubles per level. The duplication is inherent to the target language,
  so only a limit can keep a ~500-byte selector from asking for gigabytes.

## Error handling

`Error` carries enough detail to build a user-facing diagnostic:

```rust,no_run
use css_to_xpath::{css_to_xpath, Mode};

if let Err(e) = css_to_xpath("col || td", "", Mode::Generic) {
    eprintln!("{}", e.into_message("col || td"));
}
```

```text
The CSS selector "col || td" uses the `||` column combinator, which this translator does not support
```

## Minimum supported Rust version

Rust **1.88**, edition 2024 — set by the floor of the `cssparser`/`selectors`
dependency versions this crate requires.

## License

Licensed under the [MIT license](LICENSE).
