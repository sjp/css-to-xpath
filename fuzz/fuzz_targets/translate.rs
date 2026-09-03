//! Feed arbitrary text to all three translator modes.
//!
//! The crate invests heavily in bounding pathological input —
//! `MAX_NESTING_DEPTH`, `MAX_NTH_OF_DEPTH`, `MAX_NTH_OF_BYTES` and the
//! pre-parse `scan` — and this is what checks those bounds hold against
//! input nobody thought of. Four properties:
//!
//! 1. no panic, and no stack overflow on the 1 MiB stack the depth
//!    limit is sized for (hence the worker thread: libFuzzer's own
//!    thread is larger, which would hide a limit set too high);
//! 2. output stays proportionate to input;
//! 3. every successful translation is syntactically valid XPath — the
//!    same oracle `tests/xpath_validity.rs` applies to the corpus, here
//!    against inputs nobody wrote, which is where an unbalanced bracket
//!    or a precedence mistake would hide;
//! 4. the `prefix` argument does what the README says it does: it is
//!    prepended to each selector-group branch and nothing else, except
//!    on a `:scope`-anchored branch, which ignores it. Translation is
//!    also deterministic.
//!
//! Every property is checked with and without a default namespace, the
//! setting that turns every unprefixed type selector and every implicit
//! universal into a qualified name test.

#![no_main]

use css_to_xpath::{Mode, Translator, DESCENDANT_OR_SELF};
use libfuzzer_sys::fuzz_target;
use sxd_xpath::Factory;

/// The stack `MAX_NESTING_DEPTH` is sized against: the smallest the
/// crate expects to run on, not the 2 MiB Rust gives a spawned thread.
const STACK_SIZE: usize = 1024 * 1024;

/// Generous ceiling on output length: `LINEAR * input + SLACK` bytes.
///
/// Expansion is not linear in general — `:nth-child(An+B of S)` nests up
/// to `MAX_NTH_OF_DEPTH` levels, each of which may repeat its argument,
/// with `MAX_NTH_OF_BYTES` (1 MiB) capping any single `of` argument. A
/// search over deliberately adversarial selectors (nested `of S` around
/// a doubling `:is()` chain around `:required`) tops out around 5,700
/// bytes of output per byte of input. 16,384 leaves ample headroom for
/// shapes that search missed, while still failing loudly on growth that
/// is exponential in the nesting depth rather than bounded by it.
const LINEAR: usize = 16 * 1024;
const SLACK: usize = 64 * 1024;

/// Output above this is not parsed. Past a megabyte the expression is
/// the same handful of shapes repeated by a nested `of S`, so the parse
/// costs throughput without covering anything the smaller instance of
/// the same bug does not already cover.
const MAX_PARSED: usize = 1024 * 1024;

fuzz_target!(|data: &str| {
    let css = data.to_owned();
    std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(move || translate_all(&css))
        .expect("failed to spawn the worker thread")
        .join()
        .expect("the translator panicked");
});

fn translate_all(css: &str) {
    let factory = Factory::new();
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        for translator in [
            Translator::new(mode),
            Translator::new(mode).with_default_namespace_prefix("h"),
        ] {
            let bare = translate(&factory, &translator, css, "");
            assert_eq!(
                bare,
                translate(&factory, &translator, css, ""),
                "translating {css:?} twice gave different results",
            );

            let prefixed = translate(&factory, &translator, css, DESCENDANT_OR_SELF);
            match (&bare, &prefixed) {
                (Ok(bare), Ok(prefixed)) => check_prefix(bare, prefixed, DESCENDANT_OR_SELF),
                (Err(_), Err(_)) => {}
                _ => panic!("{css:?} translated under one prefix but not the other"),
            }
        }
    }
}

/// Translate, and check the length bound and XPath validity of a
/// successful result. Errors are returned as their message so that the
/// determinism check covers them too.
fn translate(
    factory: &Factory,
    translator: &Translator,
    css: &str,
    prefix: &str,
) -> Result<String, String> {
    let xpath = match translator.css_to_xpath(css, prefix) {
        Ok(xpath) => xpath,
        Err(e) => return Err(e.to_string()),
    };

    let limit = SLACK + LINEAR.saturating_mul(css.len());
    assert!(
        xpath.len() <= limit,
        "output grew to {} bytes from {} bytes of input (limit {limit})",
        xpath.len(),
        css.len(),
    );

    if xpath.len() <= MAX_PARSED {
        match factory.build(&xpath) {
            Ok(Some(_)) => {}
            Ok(None) => panic!("{css:?} (prefix {prefix:?}) produced an empty XPath"),
            Err(e) => panic!("{css:?} (prefix {prefix:?}) produced invalid XPath: {e}\n  {xpath}"),
        }
    }

    Ok(xpath)
}

/// The prefixed translation must be the bare one with `prefix` inserted
/// at the start of every selector-group branch — the `:scope`-anchored
/// branches, which anchor on `self::` instead, excepted.
fn check_prefix(bare: &str, prefixed: &str, prefix: &str) {
    let expected = branches(bare)
        .into_iter()
        .map(|branch| {
            if branch.starts_with("self::") {
                branch.to_owned()
            } else {
                format!("{prefix}{branch}")
            }
        })
        .collect::<Vec<String>>()
        .join(" | ");

    assert_eq!(
        expected, prefixed,
        "prefix {prefix:?} did more than prefix each branch of {bare:?}",
    );
}

/// Split a translation at the top-level ` | ` unions, one per
/// selector-group branch.
///
/// Depth tracking keeps out the `|` that a multi-argument `:has()` puts
/// inside a predicate, and quote tracking keeps out one that came from
/// an attribute value. XPath string literals have no escapes, so the
/// scan is exact rather than a heuristic.
fn branches(xpath: &str) -> Vec<&str> {
    let bytes = xpath.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0u32;
    let mut quote: Option<u8> = None;

    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' => quote = Some(b),
                b'[' | b'(' => depth += 1,
                b']' | b')' => depth = depth.saturating_sub(1),
                b'|' if depth == 0 => {
                    assert!(
                        i > 0 && bytes[i - 1] == b' ' && bytes.get(i + 1) == Some(&b' '),
                        "a top-level | that is not a ` | ` branch separator: {xpath}",
                    );
                    out.push(&xpath[start..i - 1]);
                    start = i + 2;
                }
                _ => {}
            },
        }
    }

    out.push(&xpath[start..]);
    out
}
