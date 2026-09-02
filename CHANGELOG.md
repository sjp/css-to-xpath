# Changelog

All notable changes to this crate are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Translated XPath is part of the contract: a change to the expression a given
selector produces is listed under **Changed**, even when the two expressions
select the same nodes, because callers compare, cache and embed the strings.

## [Unreleased]

Slated for 0.3.0, the version already set in `Cargo.toml`.

### Added

- `Error` now implements `Display` and `std::error::Error`, so it composes with
  `?`, `anyhow`, `thiserror` and friends. `Display` is a one-line form that does
  not need the selector (`invalid CSS selector at byte 4: ...`).
- `ParseErrorKind`, a crate-owned `#[non_exhaustive]` enum describing *why* a
  selector failed to parse, replacing the pre-rendered `String`. Payloads that
  echo the selector are sanitized: control characters replaced, text elided past
  40 bytes.
- `Error::message(&self, selector)`, the full multi-line form with the caret
  gutter, borrowing the error instead of consuming it.
- `Translator::mode()`, and `Debug`/`PartialEq`/`Eq`/`Hash` on `Translator`.
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
  `Error::Unsupported(String)` is now `Error::Unsupported { construct: String }`.
- **Breaking:** removed the `VERSION` constant; use `env!("CARGO_PKG_VERSION")`
  in your own crate, or read the dependency's version from Cargo metadata.
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

### Deprecated

- `Error::into_message`, which consumes the error. Use `Error::message`.

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
