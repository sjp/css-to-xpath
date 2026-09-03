# Changelog

All notable changes to this crate are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Translated XPath is part of the contract: a change to the expression a given
selector produces is listed under **Changed**, even when the two expressions
select the same nodes, because callers compare, cache and embed the strings.

## [Unreleased]

### Added

- The form-state pseudo-classes a static translation can answer, under
  `Mode::Html` and `Mode::Xhtml`: `:read-write`/`:read-only` (HTML's
  mutability over `input` and `textarea`, plus `contenteditable`
  subtrees — the two partition every element), `:default` (checked
  checkbox or radio, selected `option`, and a form's default button) and
  `:placeholder-shown`. They previously failed to parse. `:valid`,
  `:in-range` and `:indeterminate` still do, and all four join the
  never-match set under `Mode::Generic`.
- `Error` now implements `Display` and `std::error::Error`, so it composes with
  `?`, `anyhow`, `thiserror` and friends. `Display` is a one-line form that does
  not need the selector (`invalid CSS selector at byte 4: ...`).
- `ParseErrorKind`, a crate-owned `#[non_exhaustive]` enum describing *why* a
  selector failed to parse, replacing the pre-rendered `String`. Payloads that
  echo the selector are sanitized: control characters replaced, text elided past
  40 bytes.
- `Error::message(&self, selector)`, the full multi-line form with the caret
  gutter, borrowing the error instead of consuming it.
- `Translator::with_default_namespace_prefix()`, which puts unprefixed type
  selectors in a default namespace the way a stylesheet's `@namespace url(…)`
  does, and `Translator::default_namespace_prefix()`, which reads it back. The
  crate still never sees a namespace URL, so the default namespace is named by
  the prefix the output should carry: `Translator::new(Mode::Xhtml)
  .with_default_namespace_prefix("h")` translates `body > p` to `h:body/h:p`
  instead of leaving it in the null namespace, which in an XHTML, SVG or Atom
  document matches nothing. The semantics are CSS Namespaces 3's: the prefix
  reaches type selectors and the implicit universal of a compound that has none
  (`.c` becomes `h:*[…]`), but never attribute selectors, and never the
  featureless subject of an `:is()`/`:where()`/`:not()` argument; `|e` and `*|e`
  keep their meanings. Translators built without one are unaffected, so no
  existing output moves.
- `Translator::mode()`, and `Debug`/`PartialEq`/`Eq`/`Hash` on `Translator`.
- `FromStr`, `Display` and `Mode::as_str()` on `Mode`, for callers that read a
  mode from a CLI flag or a config file: `"xhtml".parse()` is `Mode::Xhtml`, the
  three names are matched ASCII case-insensitively, and nothing else is
  accepted. The failure is a new unit-struct `ParseModeError`, which implements
  `Display` and `std::error::Error`. `Default` on `Mode` (`Mode::Generic`) and
  on `Translator` (`Translator::new(Mode::Generic)`, no default namespace).
- `DESCENDANT_OR_SELF` and `WHOLE_DOCUMENT` constants for the two `prefix`
  values worth naming.
- `MAX_NESTING_DEPTH`, `MAX_NTH_OF_DEPTH` and `MAX_NTH_OF_BYTES`, so the limits
  the error messages quote can be read from the API rather than copied.
- Execution-based tests that run every translated expression against real
  documents with `sxd-xpath`, An+B property tests with `proptest`, a
  `cargo-fuzz` target, and a weekly `cargo-mutants` workflow.
- The string-pinning tests moved out of `src/lib.rs` into per-family suites
  under `tests/`, so they exercise only the public API. Each suite reports
  every failing selector by name instead of aborting on the first mismatch.
- Packaging metadata: `keywords`, `categories`, `documentation`, and an
  `include` list. The README is now the crate-level documentation.
- CI runs `cargo semver-checks check-release` on every change, and the MSRV job
  runs the test suite rather than only checking that it compiles. The lints the
  crate has adopted beyond the defaults are declared in a `[lints]` table, so
  the level is the same locally and in CI.
- An "Approximations" section in the README recording where the output is
  deliberately not what Selectors Level 4 asks for: `:lang()` as a prefix
  match rather than RFC 4647 extended filtering, Level 3 `:empty`,
  attribute-only `:checked`, `Mode::Html` lowercasing foreign content,
  form feed in class token lists, and quoted non-ASCII names. The empty
  language range `:lang("")` is listed under "Not supported". No behaviour
  changed.

### Changed

- **Breaking:** `Error` is `#[non_exhaustive]` and its variants are struct-like:
  `Error::Parse(String, u32)` is now
  `Error::Parse { kind: ParseErrorKind, offset: usize }`, and
  `Error::Unsupported(String)` is now
  `Error::Unsupported { construct: String, offset: Option<usize> }`.
- **Breaking:** `Error::Unsupported` carries the offending byte position when it
  is known, and `Error::message` then renders the same caret gutter an
  `Error::Parse` gets — so `"col || td"` points at the `||`, and a selector
  nested past `MAX_NESTING_DEPTH` points at the first parenthesis over the
  limit. `Display` gains the position too (`unsupported CSS construct at byte 4:
  ...`). The position is `None` for the constructs rejected during translation,
  whose parsed components carry no source offsets.
- The `&` nesting selector is now `Error::Unsupported { construct: "the `&`
  nesting selector", .. }` with the byte offset of the `&`, instead of whatever
  the parse tripped over next — `"&"` and `"a:is(&)"` reported "the selector is
  empty", and `"a > &"` "a combinator with nothing after it", all pointing at a
  `&` the caret line showed. A pre-parse scan finds it, exactly as it finds
  `||`. `Unsupported` is the correct variant: `&` is valid CSS in a nesting
  context, and this crate — which never sees an enclosing rule — not supporting
  it is what that variant means. A caller matching on `Error::Parse` for these
  selectors sees `Error::Unsupported` now.
- A misplaced `:scope` and a `:host()` now carry the byte offset of the
  construct, so `Error::message` points a caret at them and `Display` names the
  position (`unsupported CSS construct at byte 5: the :scope pseudo-class
  inside a functional pseudo-class`). Both were `offset: None` before. Where a
  `:scope` is supported — the leftmost compound of its group, and no deeper
  than the top level — is a lexical fact, as is the presence of a `:host()`, so
  the pre-parse scan that already finds `||` and `&` decides them too and knows
  where they are. The scan's findings are consulted only after the parse has
  succeeded, so a selector that is *also* invalid CSS keeps the parse error it
  reported before. The constructs left with no position are the ones that
  depend on what the compound resolved to (an of-type pseudo-class without a
  type, a namespace prefix that needs quoting, an `An+B of S` list over the
  limits), which the parsed components Servo hands back cannot be located from.
- **Breaking:** removed the `VERSION` constant; use `env!("CARGO_PKG_VERSION")`
  in your own crate, or read the dependency's version from Cargo metadata.
- A compound whose conditions include a never-matching `0` renders as just
  that `0`, and a condition collected twice renders once: `a:hover[x]` is
  `a[0]` rather than `a[0 and @x]`, and `a[href]:any-link` is `a[@href]`
  rather than `a[@href and @href]`. Both are simplifications of one
  conjunction, so the selected node-set is unchanged; standalone predicates
  (the `+` combinator's `[1]`) keep their own brackets.
- A repeated branch in an `:is()`/`:where()`/`:not()`/`of S` argument list
  renders once, the same rule the compound's conjunction already applied to a
  repeated condition: `:is(a, a)` is `*[self::a]` rather than
  `*[self::a or self::a]`. `X or X` selects what `X` does, so the node-set is
  unchanged; a list that folds down to one branch is no longer an or-group, so
  it also loses the parentheses it would have been given when conjoined —
  `e[foo]:is(a, a)` is `e[@foo and self::a]`.
- The `An+B` modulo offset is spaced like every other operator the crate
  emits: `a:nth-last-child(2n)` is
  `a[(count(following-sibling::*) + 1) mod 2 = 0]`, not `... +1) ...`. The
  expression is unchanged for an XPath engine; only the string differs.
- Class selectors no longer emit the redundant `@class and` guard:
  `div.warning` is
  `div[contains(concat(' ', normalize-space(@class), ' '), ' warning ')]`.
- Element-name tests inside HTML pseudo-classes are settled during translation
  rather than left to the XPath engine, so `option:checked` is
  `option[@selected]` and a name outside a pseudo-class's element set, such as
  `a:enabled`, collapses to `a[0]`.
- Those element names are matched by local name, so they see XHTML's namespaced
  elements and work with `*|input` and `h|input` subjects alike.
- `Mode::Html` compares HTML's legacy case-insensitive attribute values (`type`,
  `rel`, `lang`, ... the list HTML fixes) without regard to case, so
  `[type=CHECKBOX]` matches `<input type="checkbox">`. `Mode::Xhtml` keeps those
  values case-sensitive.
- Dependencies are caret ranges (`selectors = "0.40"`) rather than exact pins,
  so they unify with other crates in a dependency graph.
- **Breaking:** `Translator` is no longer `Copy`, since it can now own a default
  namespace prefix, and its methods take `&self` rather than `self`. It is still
  `Clone`; method calls do not have to change, but code that relied on an
  implicit copy — passing a translator by value and then using it again — needs
  a borrow or a `.clone()`.
- `:lang()` under `Mode::Html` and `Mode::Xhtml` matches a multi-subtag range by
  RFC 4647 extended filtering rather than by a dash-terminated prefix, which is
  what Selectors 4 asks for: `:lang(de-DE)` now also matches `de-Latn-DE`, and
  `:lang(zh-TW)` matches `zh-Hant-TW`. The range's first subtag must still equal
  the tag's first, and each later one must appear as a whole subtag after the
  previous match, so `:lang(de-DE)` still does not match `de-DEUTSCH` or
  `dede-DE`. The emitted XPath grows a `contains(…, substring-after(…))` step per
  subtag past the first; single-subtag ranges (`en`, `en-*`, `*`) are unchanged,
  as is `Mode::Generic`, which delegates to XPath's own prefix-matching `lang()`.
  RFC 4647's rule that a subtag may not be skipped past a singleton is not
  modelled — see the README's Approximations.
- The functional-pseudo-class nesting limit is **32** levels rather than 64, so
  the recursion it bounds fits the 1 MiB stack a library does not get to choose
  — a Windows main thread, a wasm32 module, a thread pool's worker — in an
  unoptimized build, where a level costs about 16 KB. At 64 such a build aborted
  the process at 57 levels, six short of the depth the limit promised to accept,
  which is the failure the limit exists to prevent. Selectors nested 33 to 64
  deep now return `Error::Unsupported` rather than translating; nothing
  hand-written comes close, and the length of a selector or an argument chain is
  still unlimited.

### Deprecated

- `Error::into_message`, which consumes the error. Use `Error::message`.

### Fixed

- The README listed `:dir()` under "Not supported", claiming it errors. It has
  always parsed and translated to a never-matching `[0]`, like `:hover` and
  `:visited`, because resolved directionality needs the bidi algorithm; the
  README and the `Mode` docs now say so, and record that the argument is not
  interpreted (`:dir(rtl)` and `:dir(foo)` translate alike). No behaviour
  changed.
- The README's "Testing" section described the string-pinning tests as unit
  tests under `src/`, which they stopped being when they moved into the
  per-family suites under `tests/`; it now names those suites and the `Cases`
  checker that drives them, and quotes the fuzz target's actual 1 MiB stack
  rather than the 2 MiB it used before the nesting limit was resized.
- The README called the form feed that CSS splits `class` tokens on, and
  `normalize-space` does not, something the target language cannot express.
  It can: a `translate()` mapping U+000C to a space, wrapped around the
  attribute before `normalize-space`, closes the gap. The bullet now records
  the trade that was actually made and kept — XPath 1.0 string literals have
  no escape syntax, so the fix would put a raw control character in the output
  of the most common construct the crate emits, to serve a `class` attribute
  almost nobody writes — and a `tests/semantics.rs` case pins the divergence
  so the decision cannot be reversed by accident. No behaviour changed.
- The README did not mention that css-syntax-3 auto-closes an open block,
  function or string at end of input, so a truncated selector translates as
  its closed form (`a[b` is `a[@b]`) rather than erroring. "Error handling"
  now says so. No behaviour changed.

## [0.2.0] - 2026-09-02

### Added

- `:lang()` in `Mode::Xhtml` reads `xml:lang` as well as `lang`, preferring
  `xml:lang` when both sit on the nearest ancestor, as HTML's language
  determination requires.
- Hard limits on pathological input, each reported as an error rather than
  allowed to blow up: pseudo-class nesting depth (64 levels), `An+B of S`
  nesting depth, and total generated size for `of S` expansion.
- The error message for a parse failure gained a caret gutter, aligned by
  display width rather than byte count and bounded to 72 columns.

### Changed

- **Breaking:** the translation mode is a `Mode` enum rather than a string, so a
  typo is a compile error instead of a runtime one.
- **Breaking:** an unprefixed type name inside a functional pseudo-class
  argument now carries the same null-namespace meaning it has everywhere else,
  translating to a `self::` test: `:is(p)` is `*[self::p]`, `:not(p)` is
  `*[not(self::p)]`. Previously the argument was matched by name only.
- `:enabled`, `:disabled`, `:checked` and `:link` are limited to the element
  sets HTML defines them over; anything else no longer matches.
- An `<option>` inside a disabled `<optgroup>` is `:disabled`.
- Element and attribute names, and `:lang()` ranges, are lowercased with ASCII
  case folding rather than Unicode, matching how HTML compares them.
- An empty `:is()` or `:where()` argument list is accepted and never matches,
  instead of being a parse error.
- Namespace prefixes are kept in the node test for names that need quoting.
- Long combinator chains are built in a single pass instead of being re-rendered
  per combinator.
- Upgraded to `selectors` 0.40.

### Fixed

- `:lang()` arguments must now be a comma-separated list of valid language
  ranges, and `:lang(*)` requires a known, non-empty language. Inputs that were
  silently accepted before are errors.
- Of-type pseudo-classes on a prefixed wildcard (`ns|*:first-of-type`) are
  rejected: a prefixed wildcard names a namespace, not a type, so counting its
  siblings would answer the wrong question.
- A namespace prefix that would still require quoting after translation is an
  error rather than invalid XPath.

## [0.1.0] - 2026-07-20

Initial release, migrated from the `selectrs` package.

[Unreleased]: https://github.com/sjp/css-to-xpath/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/sjp/css-to-xpath/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sjp/css-to-xpath/releases/tag/v0.1.0
