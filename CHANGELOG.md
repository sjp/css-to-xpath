# Changelog

All notable changes to this crate are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this crate adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Translated XPath is part of the contract: a change to the expression a given
selector produces is listed under **Changed**, even when the two expressions
select the same nodes, because callers compare, cache and embed the strings.

## [Unreleased]

### Fixed

- An element *named* `*` — written with an escape, as `\2a` — is no longer
  translated as the universal selector when a namespace constraint is written
  in front of it. An escape only ever produces an `<ident>` (css-syntax-3
  §4.3.7), so `\2a` is a type selector whose name happens to be the character
  `*`; only the delimiter `*` is the universal selector (Selectors 4
  §5.1–5.2). The translator told the two apart by comparing the
  name to `"*"`, which read `*|\2a` as `*|*` (every element in the document,
  where the selector asks for elements named `*` in any namespace) and `|\2a`
  as `|*` (every element in no namespace). They are now `*[local-name() = '*']`
  and `*[name() = '*' and namespace-uri() = '']`, matching what the unprefixed
  `\2a` and the prefixed `ns|\2a` already produced. For the same reason
  `*|\2a:first-of-type` — refused as "an of-type pseudo-class on the universal
  selector `*`" — now translates, counting siblings by the name as any other
  type selector does. The universal selector itself is unchanged in every
  namespace form (`*`, `|*`, `*|*`, `ns|*`), and `\2a` in the *prefix*
  position is still "a namespace prefix that is not an XPath name (`*`)".

## [0.5.1] - 2026-09-05

### Added

- The empty language range `:lang("")` now has its Selectors 4 meaning — "the
  element's language is not known" — where it was rejected as a malformed
  range. It is the complement of `:lang(*)`, so it translates to that range's
  test negated: `e:lang("")` is
  `e[not(ancestor-or-self::*[@lang][1][string-length(@lang) > 0])]` under
  `Mode::Html`, and the same shape over `@xml:lang` under `Mode::Generic` and
  over both attributes under `Mode::Xhtml`. An empty attribute counts as no
  language, as it does for `:lang(*)`: an element under `lang=""` matches
  `:lang("")` rather than inheriting the tag above it. XPath's own `lang()`
  cannot express this, so `Mode::Generic` walks the language attribute
  directly, as it already did for `:lang(*)`.

### Changed

- A `:lang()` language range that no language tag could match — an empty
  subtag (`en-`, `--x`) or a `*` glued to one rather than standing as a whole
  subtag (`en*`, `*en`) — is now refused by the translator, which names it:
  `` the :lang() language range "en-" (an empty subtag, which a language range
  cannot have) ``. These were rejected by the argument grammar before, as
  `` `lang` is not a supported pseudo-class or pseudo-element `` — a message
  that named neither the range nor what was wrong with it. So they are now
  `Error::Unsupported` (with no offset) where they were `Error::Parse` (with a
  caret on `lang`); the set of selectors that error is unchanged apart from
  `:lang("")`, which now translates. The grammar still decides what it alone
  can: whether the tokens assemble into ranges at all, which is where
  `:lang()`, `:lang(5)`, `:lang(en fr)` and a stray comma are still refused.
- Every message payload echoed from the selector is bounded by one rule: a
  `:lang()` range is now sanitized and elided past 40 bytes, as a token or
  pseudo-class name in a parse error already was. This is visible only in the
  `:lang()` errors above and in the existing "a wildcard outside the final
  subtag" one, whose range came through raw however long it was.

### Fixed

- A `:lang()` range built from more than one token is no longer refused when
  it follows another range in the list: `:lang(en, *-CH)` and
  `:lang(en, de-*)` now parse, as `:lang(*-CH, en)` and `:lang(de-*, en)`
  already did. The argument grammar reads whitespace as a range terminator,
  and the flag recording it was not cleared once the next range had started,
  so the space after a comma went on separating the tokens of every range
  after it — making the multi-token ranges (`*-CH`, which the tokenizer
  splits into `*` and `-CH`) look like two ranges with no comma between them.
  Whitespace *inside* a range is still an error: `:lang(en, * -CH)` and
  `:lang(en fr)` are refused as before.

## [0.5.0] - 2026-09-04

### Changed

- A compound that can never match inside a complex functional-pseudo-class
  argument now ends the chain, instead of leaving the reversed-axis test that
  carried the compounds to its left in the output: `e:is(f > g:is())` is
  `e[0]` rather than `e[0 and parent::*[self::f]]`, and
  `li:nth-child(2 of f > g:is())` is `li[0]` rather than an expression naming
  the dead chain twice, once inside a `count()` over every preceding sibling.
  Both selected the same (empty) node set before, so no caller gets a
  different answer. This is the fold `XPathExpr::condition` already applied to
  a single compound's own conjunction (`a:hover[x]` is `a[0]`), reaching the
  conjunction that `Translator::argument_condition` assembles across
  compounds; it applies through `:is()`, `:where()`, `:not()` and the nth
  `of S` lists, while `:has()` builds a path and was never affected. Only the
  compounds to the *left* of the dead one go — `e:is(f > g:is() > h)` keeps
  its `self::h` and is `e[self::h and parent::*[0]]` — and a `0` that is
  already alone inside its axis bracket stays put, so `e:is(f:is() > g)`
  remains `e[self::g and parent::*[0]]` and still shows which compound is the
  impossible one.

## [0.4.0] - 2026-09-04

### Added

- `:lang()` under `Mode::Html` and `Mode::Xhtml` now accepts a `*` subtag in any
  position, which is what Selectors 4's RFC 4647 extended filtering allows:
  `:lang(*-CH)` — "any language as written in Switzerland" — was rejected as "a
  wildcard outside the final subtag", and now matches `de-CH` and `fr-Latn-CH`.
  A leading `*` stands for the tag's first subtag, so the walk starts one
  subtag in rather than anchoring with `starts-with`, and `:lang(*-CH)`
  therefore does not match the tag `ch` itself. A `*` anywhere else moves past
  nothing — every range subtag after the first is already searched for through
  the whole remaining tag — so it drops out of the translation: `:lang(de-*-DE)`
  emits exactly what `:lang(de-DE)` emits, as `:lang(en-*)` already emitted what
  `:lang(en)` does. `Mode::Generic` still takes only `*` and a final `en-*`,
  since it hands the range to XPath's `lang()`, a prefix match with nowhere to
  put the rest; its error now says so.

### Changed

- An `option` or `optgroup` inside a disabled `select` is now `:disabled` rather
  than `:enabled`, and an `option` is now disabled by the nearest `optgroup`
  above it rather than only by a parent one. Both are rules HTML added to
  "actually disabled" after this translation was written: an `optgroup` or
  `option` "whose nearest ancestor `select` is disabled" is actually disabled,
  and an `option` is disabled by the nearest ancestor that settles it — the
  walk ends at a `select`, `hr`, `datalist` or `option`, so an `option` in a
  `<datalist>` inside a disabled `select` stays `:enabled`, and an `option`
  nested below its `optgroup` rather than directly under it is disabled. This
  changes what `:disabled`, `:enabled`, `option:disabled`, `option:enabled`,
  `optgroup:disabled` and `optgroup:enabled` select over a document with a
  disabled `select` in it, which is the ordinary way a whole control is
  switched off, as well as the expressions they translate to. One corner is
  approximated, as the `legend` counting already is: HTML's walk also gives up
  once it has passed a second `optgroup`, which XPath cannot count, so an
  `option` two `optgroup`s deep — non-conforming markup — inside a disabled
  `select` is `:disabled` here.
- The error for a namespace prefix that is not an XML `NCName` now names the
  rule rather than an operation the prefix cannot have: `\31 ns|div` gives
  `` a namespace prefix that is not an XPath name (`1ns`) `` where it
  previously gave "a namespace prefix that needs quoting". Quoting is what a
  *local* name that cannot be a node test gets — it folds into a `name()` or
  `local-name()` comparison — so the old wording named the very fallback the
  prefix is being refused for. The README now records why that fallback stops
  at the prefix: it is an exact rewrite of a node test, whereas comparing a
  whole `prefix:name` against `name()` (which some other translators emit
  here) would match on how the *document* spells its prefix, where every
  prefix this crate emits is resolved by what the caller bound it to — and
  nothing the caller binds could help, since an XPath expression cannot name
  such a prefix at all. Which selectors translate is unchanged.
- The `Mode::Generic` error for a wildcard it cannot place now names the reason
  rather than only the placement: `` the :lang() language range "*-CH" (a
  wildcard outside the final subtag, which XPath's lang() cannot express) ``.
  The range is no longer an error everywhere, so the message has to say which
  translator it is beyond.
- The caret in a parse error's message now points at the token the message
  names, rather than at the position the parse stopped on. `[a=b c]` reports
  ``unexpected `c` `` at byte 5, the `c`, where it previously reported byte 4,
  the space in front of it; `:nth-child(foo)` reports ``unexpected `foo` `` at
  byte 11 rather than byte 14, the `)` past it; and `a::part(b)` reports
  `` `part` is not a supported pseudo-class or pseudo-element `` at byte 3, the
  name, rather than byte 8, the argument. `Error::Parse`'s `offset` is
  therefore usable as a span: where the message echoes a piece of the selector,
  the offset is where that piece was written. Positions already on what they
  named — `div.-5`, reported on the `-5` rather than on the `.` — do not move.
- A selector group whose first token cannot start a compound is now reported by
  that token rather than as `ParseErrorKind::EmptySelector`: `#1abc` gives
  ``unexpected `#1abc` `` where it previously gave "the selector is empty", which
  sent the reader looking for a missing selector instead of at the
  digit-leading identifier that is there. It is also the message `p#1abc` has
  always given, so the same mistake no longer reports two ways depending on
  whether a type selector precedes it. `EmptySelector` now means only what it
  says: `""`, `"  "`, `"a, , b"`.
- The README listed `:link`/`:any-link` as matching an `a` or `area` with an
  `href` without saying that the `link` element, which also carries an `href`,
  is outside that set — the difference a reader hits first, since other
  translators (selectr) do match it. It now says so, and the exclusion is
  quoted from HTML's own wording ("all `a` elements that have an `href`
  attribute, and all `area` elements that have an `href` attribute, must match
  one of :link and :visited") where the translation makes it, so the decision
  is not re-opened from memory. Both HTML fixtures grew a `<link href>` in a
  `head`, pinning at the document level what `link:link` → `link[0]` already
  pinned as a string. No behaviour changed.
- The README and the code left the `:disabled`/`:enabled` element set open to
  being read as type-sensitive, since it sits beside `:required`,
  `:read-write` and `:placeholder-shown`, where an `input`'s type does decide.
  It is not: HTML's `disabled` attribute has no "Applies to" list of type
  states, so `<input type="hidden" disabled>` is `:disabled`, and neither half
  of the pair emits a `@type` test at all. Other translators (selectr) drop
  Hidden from both halves, a carve-out the spec does not make. The README now
  says the set is by element alone, the constant holding it says why, and
  `tests/html_mode.rs` asserts the absence of the `@type` test rather than
  leaving it to be noticed. Both HTML fixtures grew a disabled hidden `input`
  and the differential fixture a disabled Hidden/RANGE pair, so a document
  pins it too. No behaviour changed.
- `[attr$=value]` now spaces the subtraction in the expression it translates
  to: `a[href$="pdf"]` gives `substring(@href, string-length(@href) - 2) =
  'pdf'` where it previously gave `string-length(@href)-2`. The two parse
  identically as XPath, so only the string differs — but the string is the
  contract. This was the crate's last unspaced binary operator; every other one
  it emits already carried a space on both sides. selectr 0.7-0 made the same
  change, so the two remain comparable string-for-string.

## [0.3.0] - 2026-09-03

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
- Namespace prefixes are now held to the XML `NCName` production instead of
  the ASCII-only test used for local names, so a non-ASCII prefix translates
  rather than erroring: `nsé|div` becomes `nsé:div`, `[nsé|href]` becomes
  `*[@nsé:href]`. The rule for *local* names is unchanged — one that cannot
  be a node test still folds into `local-name()`, which is why being
  conservative there costs nothing and being conservative about a prefix,
  which has no such fallback, cost fidelity. A prefix that is not a name at
  all (`\31 ns|div`) is still an `Unsupported` error, and no selector that
  translated before translates differently. The accepted set is XML 1.0's
  original `Name` tables, the ones XPath 1.0 cites, which are a subset of
  the Fifth Edition set some engines use: the output parses under either
  reading, checked through both sxd-xpath and libxml2.

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

[Unreleased]: https://github.com/sjp/css-to-xpath/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/sjp/css-to-xpath/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/sjp/css-to-xpath/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/sjp/css-to-xpath/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/sjp/css-to-xpath/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sjp/css-to-xpath/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sjp/css-to-xpath/releases/tag/v0.1.0
