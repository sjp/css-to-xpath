# css-to-xpath

[![Crates.io](https://img.shields.io/crates/v/css-to-xpath.svg)](https://crates.io/crates/css-to-xpath)
[![Docs.rs](https://docs.rs/css-to-xpath/badge.svg)](https://docs.rs/css-to-xpath)
[![CI](https://github.com/sjp/css-to-xpath/actions/workflows/ci.yml/badge.svg)](https://github.com/sjp/css-to-xpath/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/sjp/css-to-xpath/blob/master/LICENSE)

Translate CSS selectors to XPath 1.0 expressions.

`css-to-xpath` parses CSS with [Servo's](https://github.com/servo/servo)
own `selectors` and `cssparser` crates. It uses this to construct XPath 1.0 expressions so that they can be evaluated using XML libraries such as `libxml2`.

## Installation

```sh
cargo add css-to-xpath
```

```toml
[dependencies]
css-to-xpath = "0.3"
```

## Quick start

```rust
use css_to_xpath::{css_to_xpath, Mode};

// mode: Mode::Generic | Mode::Html | Mode::Xhtml; prefix: prepended to the result.
assert_eq!(
    css_to_xpath("div.warning > a", "", Mode::Generic).unwrap(),
    "div[contains(concat(' ', normalize-space(@class), ' '), ' warning ')]/a"
);

assert_eq!(
    css_to_xpath("li:nth-child(odd)", "", Mode::Generic).unwrap(),
    "li[count(preceding-sibling::*) mod 2 = 0]"
);
```

For repeated translations, build a `Translator` once and reuse it:

```rust
use css_to_xpath::{Mode, Translator};

let translator = Translator::new(Mode::Generic);
let xpath = translator.css_to_xpath("e:has(> .foo)", "").unwrap();
assert_eq!(
    xpath,
    "e[child::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]"
);
```

## Translator flavours

`Translator::new` takes one of three `Mode` variants:

- **`Mode::Generic`** — plain CSS/XPath semantics, case-sensitive names, no
  HTML-specific pseudo-classes.
- **`Mode::Html`** — lowercases element and attribute names (as HTML parsing
  does), compares HTML's legacy case-insensitive attribute values
  (`type`, `rel`, `lang`, `checked`, … — the list HTML fixes) without
  regard to case, so `[type=CHECKBOX]` matches `<input type="checkbox">`,
  and gives dynamic-seeming pseudo-classes their static HTML
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
  - `:read-write`/`:read-only` — HTML's mutability: an `input` of a type
    `readonly` applies to (an invalid or missing `type` is `text`, which
    it does), or a `textarea`, that is neither `readonly` nor disabled —
    plus any element inside a `contenteditable` subtree, control or not.
    `:read-only` is Selectors 4's complement of the whole expression, so
    unlike `:disabled`/`:enabled` the two partition *every* element.
  - `:default` — a checked checkbox or radio `input`, a selected
    `option`, and a form's default button: the first submit button in it,
    where a `button` with no `type` is one.
  - `:placeholder-shown` — an `input` or `textarea` carrying a non-empty
    `placeholder` its type allows, with no value in the markup.
  - `:lang()` — nearest `@lang` ancestor, case-folded prefix match.
- **`Mode::Xhtml`** — the same HTML pseudo-class semantics as `Mode::Html`,
  but preserves case (XHTML is XML, so both names and those attribute
  values are case-sensitive) and reads `xml:lang` as well as `lang` for
  `:lang()`, preferring `xml:lang` when both are on the nearest ancestor
  (HTML's language determination).
  The element names *inside* these pseudo-classes — the `fieldset`
  ancestor, the parent `optgroup`, the `a`/`area` of `:link` — are matched
  by local name, so they see XHTML's namespaced elements and work with
  `*|input` and `h|input` subjects alike. Only the names you write follow
  the namespace rule below. When the compound names its element, those
  local-name tests are settled during translation rather than by the
  XPath engine: `option:checked` is `option[@selected]`, and a name
  outside the pseudo-class's element set (`a:enabled`) leaves `a[0]`.

Pseudo-classes with no static equivalent (`:hover`, `:visited`,
`:focus`, …) always translate to an unmatchable `[0]` rather than
erroring, in every flavour. `:dir()` is one of them: it selects on
*resolved* directionality, which needs the bidi algorithm, and the
nearest-`@dir`-ancestor approximation was rejected because it gets
`dir="auto"`, `bdi`, and HTML's invalid-value-means-inherit rule wrong.
Its argument is parsed — exactly one identifier, as Selectors 4 spells
it — but never interpreted, so `:dir(ltr)`, `:dir(rtl)` and
`:dir(anything)` all translate alike.

## The `prefix` argument

`prefix` is prepended to each translated selector-group branch — pass
`"descendant-or-self::"` to search an entire subtree, or `""` for a bare
expression:

```rust
use css_to_xpath::{css_to_xpath, Mode};

assert_eq!(
    css_to_xpath("a, b", "descendant-or-self::", Mode::Generic).unwrap(),
    "descendant-or-self::a | descendant-or-self::b"
);
```

A selector group anchored on `:scope` ignores `prefix` and instead
anchors on the `self::` axis, since `:scope` names the context node the
XPath is evaluated from:

```rust
use css_to_xpath::{css_to_xpath, Mode};

assert_eq!(
    css_to_xpath(":scope > a", "descendant-or-self::", Mode::Generic).unwrap(),
    "self::*/a"
);
```

`prefix` is prepended verbatim and is not validated, so it has to end in
something a node test can follow: an axis or a step separator. The two
that come up are exported as constants — `DESCENDANT_OR_SELF`
(`"descendant-or-self::"`, the context node's subtree) and
`WHOLE_DOCUMENT` (`"//"`, the whole document wherever the expression is
evaluated from). A prefix ending anywhere else silently produces a
different expression: `"/html/body "` yields `/html/body div`, which
XPath reads as a division, not a path.

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
  it need it registered. `:empty` and `:lang()` both follow Level 3
  rather than Level 4 — see [Approximations](#approximations).
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

```rust
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

```rust
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
- Pseudo-classes outside the never-match allow-list, such as `:valid`,
  `:in-range` and `:indeterminate` — these error instead of silently
  matching nothing, so typos stay loud. `:indeterminate` is among them
  because only its `progress` arm is in the tree: a checkbox's
  indeterminate flag is set through the DOM and never appears in markup,
  and "no other radio with this name in this form" would need a predicate
  to refer to the element being matched from inside a nested one, which
  XPath 1.0 cannot do.
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
- The empty language range `:lang("")`, which Level 4 defines as matching
  only elements whose language is *not* tagged. It is rejected with the
  other malformed ranges (`en-`, `--x`, `en*`) rather than given that
  meaning; the ones that are supported are described below.
- Functional pseudo-classes (`:is()`, `:not()`, `:where()`, `:has()`,
  `:nth-child(… of S)`) nested more than **32** levels deep. Parsing and
  translating both recurse once per level, so the depth is capped to turn
  a pathological selector into an error instead of a stack overflow. The
  cap is sized to fit a 1 MiB stack — a Windows main thread, a wasm32
  module, a thread pool's worker — in an unoptimized build, the most
  expensive combination. Nothing hand-written comes close; only the
  nesting depth is limited, not the length of a selector or of an
  argument chain. The value is exported as `MAX_NESTING_DEPTH`.
- `:nth-child(… of S)` / `:nth-last-child(… of S)` nested more than **8**
  levels deep, or a single `of S` list translating to more than **1 MiB**.
  XPath 1.0 has no variables, so `S` has to be written out twice — once to
  filter the siblings being counted, once to constrain the element being
  matched — and a nested `of S` lands in both copies, so the output
  doubles per level. The duplication is inherent to the target language,
  so only a limit can keep a ~500-byte selector from asking for
  gigabytes. The two values are exported as `MAX_NTH_OF_DEPTH` and
  `MAX_NTH_OF_BYTES`.

## Approximations

These translate to something useful but not to exactly what Selectors
Level 4 asks for, because XPath 1.0 — or a static translation of any
kind — cannot reach the spec's answer. They are listed here so the
contract stays honest.

- **`:lang()` is a prefix match, not RFC 4647 extended filtering.** The
  range is compared against the language tag up to a subtag boundary, the
  Level 3 / `[lang|=…]` rule, so `:lang(de-DE)` matches `de-DE` and
  `de-DE-1996` but not `de-Latn-DE`, which Level 4 requires it to match:
  the wildcards Level 4 implies *between* subtags are not modelled. A
  written wildcard is allowed only as the whole range (`*`) or as the
  final subtag (`en-*`); an interior one (`de-*-DE`) errors rather than
  quietly matching the wrong set. Single-subtag ranges — `en`, `en-*`,
  `*`, the common case — are unaffected. Under `Mode::Generic` the test
  is XPath's own `lang()`, which is a prefix match in the same way.
- **`:empty` follows Level 3, so white space counts.** `e:empty` is
  `e[not(*) and not(string-length())]`, and `<p> </p>` is therefore not
  empty. Level 4 ignores document white space; browsers still ship the
  Level 3 behaviour, and so does this crate.
- **`:checked` reads attributes only.** An `<option>` can be selected
  with no `selected` attribute — the first option of a single-select
  with none marked — and only one radio per group can really be checked.
  Neither fact is visible to a translation that has only the document
  tree to work with.
- **`:placeholder-shown` answers for the initial value.** A document
  records the value a control *starts* with, so an `input` the user has
  since typed into still counts as showing its placeholder. This is
  `:checked` reading `@checked` one step further out: the markup is all
  a static translation has.
- **`:default` takes the form owner to be the nearest ancestor `form`.**
  That is what it is for every control written inside its form, which the
  default-button arm then finds by tree order. A control associated by a
  `form="id"` attribute instead — to a form it is not inside, or to none —
  is not followed, so in markup that uses `form=` the arm can name the
  wrong button. The checked-`input` and selected-`option` arms are exact.
- **Editability is read from `contenteditable` alone.** `:read-write`
  resolves the nearest ancestor-or-self that *sets* a `contenteditable`
  state (`inherit`, an invalid value, or no attribute leaves the element
  inheriting), which is the whole story in markup. A document put into
  `designMode` from script is editable with nothing in the tree to say so.
- **`Mode::Html` lowercases foreign content too.** `svg|linearGradient`
  becomes `svg:lineargradient`, which is right for libxml2's HTML
  parser, since it lowercases every name it sees. An HTML5 parser
  (html5ever, a browser) restores the camelCase SVG and MathML names
  instead, so `Mode::Html` is aimed at libxml2-style trees.
- **Class matching splits on XML white space.** `.foo` is
  `contains(concat(' ', normalize-space(@class), ' '), ' foo ')`, and
  `normalize-space` counts space, tab, CR and LF, while HTML's
  space-separated tokens also include the form feed U+000C. A `class`
  attribute that separates two tokens with a form feed keeps them joined
  here.
- **Non-ASCII names are quoted, and non-ASCII prefixes are rejected.** A
  name is written into the node test directly only if it is ASCII
  letters, digits, `_`, `.` or `-`, so `é` folds into the conservative
  `*[name() = 'é' and namespace-uri() = '']`. A namespace *prefix* gets
  no such fallback: `nsé|div` is an error under the rule above, even
  though `nsé` is a valid XML `NCName` and would be a valid XPath prefix.

## Error handling

`Error` implements `Display` and `std::error::Error`, so it propagates
through `?` into `Box<dyn Error>`, `anyhow::Error`, or a `thiserror`
`#[from]` field with no wrapper of its own:

```rust
use css_to_xpath::{css_to_xpath, Mode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let xpath = css_to_xpath("div > p", "", Mode::Generic)?;
    println!("{xpath}");
    Ok(())
}
```

`Display` is a one-line summary that needs nothing but the error, so an
error that has travelled a few layers can still be printed:

```text
invalid CSS selector at byte 6: a combinator with nothing after it
unsupported CSS construct: the `||` column combinator
```

A caller that still holds the selector can render the fuller diagnostic
with `Error::message`, which quotes the selector and — for a parse error
— points a caret at the offending position:

```rust
use css_to_xpath::{css_to_xpath, Mode};

let selector = "col || td";
if let Err(e) = css_to_xpath(selector, "", Mode::Generic) {
    eprintln!("{}", e.message(selector));
}
```

```text
The CSS selector "col || td" uses the `||` column combinator, which this translator does not support
```

```text
Unable to parse the CSS selector "div > ": a combinator with nothing after it
  |
  | div > 
  |       ^
```

The two variants say whose rules were broken. `Error::Parse { kind,
offset }` is a selector CSS itself rejects, with `kind` a
`ParseErrorKind` of this crate's own — never a `Debug` rendering of a
dependency's internal error — and `offset` the byte position the caret
points at. `Error::Unsupported { construct }` is a valid selector this
crate declines to approximate. Both are `#[non_exhaustive]`.

One class of malformed input is *not* an error: css-syntax-3 closes an
open block, function or string implicitly at end of input, so a truncated
selector translates as though it had been closed. `a[b` is `a[@b]`,
`a[b="x` is `a[@b = 'x']`, `:is(a` is `*[self::a]`, and `a /* comment`
is `a`. Nothing here departs from the spec, but a caller whose selector
can arrive truncated — a cut-off config value, a length-limited form
field — gets a plausible XPath rather than a complaint, and should check
the input's length itself if that matters.

## Testing

Five layers, all run by `cargo test`:

- **Output pinning** (`tests/`) pins the exact XPath string each
  selector translates to — the output contract — through the public API
  only, in per-family suites: `selectors.rs`, `names.rs`,
  `attributes.rs`, `nth.rs`, `functional_pseudos.rs`, `scope.rs`,
  `lang.rs`, `html_mode.rs`, `limits.rs`, `errors.rs` and `api.rs`. The
  `Cases` checker in `tests/cases/mod.rs` drives them: it names the
  selector behind a mismatch and reports every mismatch in a family
  instead of aborting at the first. The unit tests left in `src/` cover
  internal helpers the public API does not reach directly.
- **Syntactic validity** (`tests/xpath_validity.rs`) re-translates every
  selector in the shared corpus (`tests/corpus/selectors.txt`) in all
  three modes, with and without a prefix, and parses the result with
  [`sxd-xpath`](https://crates.io/crates/sxd-xpath). An unbalanced
  bracket or a precedence mistake fails here even if the pinned string
  matches.
- **Semantics** (`tests/semantics.rs`) *evaluates* the translated XPath
  against the fixture documents in `tests/fixtures/` and compares the
  selected element ids against what the CSS selector should match. The
  expectations come from the CSS semantics and the document, not from
  the translator's own output. `tests/fixtures/html.xml` is libxml2's
  HTML parse tree written out as XML — lowercased names, no namespaces —
  so a pure-Rust XML parser can stand in for it.
- **Properties** (`tests/nth_property.rs`) generate `An+B`, `An+B of S`
  and sibling counts with [`proptest`](https://crates.io/crates/proptest)
  and check the selected positions against the definition of `An+B`.
- **Differential** (`tests/differential.rs`) checks the translation
  against a second implementation rather than against an expectation
  someone wrote down. The `selectors` crate this one parses with also
  ships a matcher, so the test implements its `Element` trait over the
  same fixture tree the XPath is evaluated on and requires the two
  answers to agree. A `proptest` grammar generates the selectors —
  compounds, the four combinators, the nth family including `of S`,
  `:is()`/`:where()`/`:not()`/`:has()`, `:root`, `:empty` — while the
  attribute and of-type shapes, being small finite cross-products, are
  exhausted rather than sampled. `Mode::Generic` only, and only shapes
  the translation renders exactly: the one divergence
  (`*|e:first-of-type` counts siblings by local name, since XPath 1.0
  cannot compare a sibling's namespace against the subject's) is pinned
  as a test of its own.

Fuzzing lives in `fuzz/` and needs
[`cargo-fuzz`](https://crates.io/crates/cargo-fuzz) and a nightly
toolchain:

```sh
./fuzz/seed-corpus.sh
cargo +nightly fuzz run translate -- -max_total_time=60 -max_len=4096
```

The target runs all three modes on a thread with the 1 MiB stack the
nesting limit is sized for, and asserts four properties: no input
panics, output length stays proportionate to input, every successful
translation parses as XPath (the validity oracle above, against inputs
nobody wrote), and translation is deterministic and prefix-independent —
the prefixed output is the bare one with the prefix inserted at the
start of each branch, `:scope`-anchored branches excepted. CI runs a
two-minute pass on every change; `cargo-mutants` runs weekly.

## Minimum supported Rust version

Rust **1.88**, edition 2024 — set by the floor of the `cssparser`/`selectors`
dependency versions this crate requires.

## Changelog

Release notes, including every change to the XPath a selector translates to,
are in [CHANGELOG.md](https://github.com/sjp/css-to-xpath/blob/master/CHANGELOG.md).
The procedure for cutting a release is
[RELEASING.md](https://github.com/sjp/css-to-xpath/blob/master/RELEASING.md).

## License

Licensed under the [MIT license](https://github.com/sjp/css-to-xpath/blob/master/LICENSE).
