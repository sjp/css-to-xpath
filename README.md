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
css-to-xpath = "0.1"
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
  meaning where one exists: `:link`/`:any-link` (has `href`),
  `:checked`, `:disabled`/`:enabled` (including the fieldset/legend
  "actually disabled" carve-out), `:required`/`:optional`, and
  `:lang()` (nearest `@lang` ancestor, case-folded prefix match).
- **`Mode::Xhtml`** — the same HTML pseudo-class semantics as `Mode::Html`,
  but preserves case (XHTML is XML, so names are case-sensitive).

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
  relative-selector leading combinators inside `:has()`.
- `:scope`, `:root`, `:empty`, `:lang()`.
- The `Mode::Html`/`Mode::Xhtml` form and link pseudo-classes listed above.

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
- of-type pseudos (`:first-of-type`, `:nth-of-type()`, …) on a bare `*`
  or implicit-type compound: XPath 1.0 cannot compare a sibling's name
  against the matched element's own.
- Nested `:has()`, `:host`, and the `&` parent selector.
- `:scope` outside the leftmost compound, or inside a functional
  pseudo-class argument.

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
dependency versions this crate pins.

## License

Licensed under the [MIT license](LICENSE).
