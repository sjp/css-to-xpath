//! Every expression this crate emits must be syntactically valid XPath
//! 1.0.
//!
//! The string-pinning suites (`selectors.rs`, `nth.rs`, `html_mode.rs`
//! and the rest) pin output strings, which pins the translation but says
//! nothing about whether the string is an XPath at all: an unbalanced
//! bracket, a bad literal or a precedence mistake would sail through.
//! Here every selector in the shared corpus (which is those same pinned
//! selectors, plus the ones `semantics.rs` evaluates and the README's
//! examples) is translated in all three modes, with and without a path
//! prefix, and the result is handed to sxd-xpath's parser.
//!
//! The suites themselves assert that the corpus holds every selector
//! they use — see `tests/corpus/mod.rs` — so this oracle's input cannot
//! silently fall behind them.

mod common;

use common::corpus::{self, selectors};
use common::{MODES, mode_name};
use css_to_xpath::Translator;
use sxd_xpath::Factory;

/// A floor, not a target: it exists so that a regression turning every
/// translation into an error cannot make this test vacuously pass. The
/// corpus deliberately contains invalid selectors too, so the count is
/// well below `corpus × modes × prefixes`.
const MIN_TRANSLATIONS: usize = 2_400;

#[test]
fn every_translation_parses_as_xpath() {
    let factory = Factory::new();
    let mut translated = 0usize;

    for css in selectors() {
        for mode in MODES {
            for prefix in ["", "descendant-or-self::", "//"] {
                let Ok(xpath) = Translator::new(mode).css_to_xpath(css, prefix) else {
                    continue;
                };
                translated += 1;

                match factory.build(&xpath) {
                    Ok(Some(_)) => {}
                    Ok(None) => panic!(
                        "{} mode: {css:?} (prefix {prefix:?}) produced an empty XPath",
                        mode_name(mode)
                    ),
                    Err(e) => panic!(
                        "{} mode: {css:?} (prefix {prefix:?}) produced invalid XPath: {e}\n  {xpath}",
                        mode_name(mode)
                    ),
                }
            }
        }
    }

    assert!(
        translated >= MIN_TRANSLATIONS,
        "only {translated} corpus entries translated, expected at least {MIN_TRANSLATIONS}"
    );
}

/// The same oracle with a default namespace set, which qualifies every
/// unprefixed type selector and every implicit universal — a name test
/// the corpus otherwise only reaches through a written `ns|e`.
#[test]
fn every_default_namespace_translation_parses_as_xpath() {
    let factory = Factory::new();
    let mut translated = 0usize;

    for css in selectors() {
        for mode in MODES {
            let translator = Translator::new(mode).with_default_namespace_prefix("h");
            let Ok(xpath) = translator.css_to_xpath(css, "") else {
                continue;
            };
            translated += 1;

            match factory.build(&xpath) {
                Ok(Some(_)) => {}
                Ok(None) => panic!(
                    "{} mode: {css:?} produced an empty XPath under a default namespace",
                    mode_name(mode)
                ),
                Err(e) => panic!(
                    "{} mode: {css:?} produced invalid XPath under a default namespace: {e}\n  {xpath}",
                    mode_name(mode)
                ),
            }
        }
    }

    assert!(
        translated >= MIN_TRANSLATIONS / 3,
        "only {translated} corpus entries translated, expected at least {}",
        MIN_TRANSLATIONS / 3
    );
}

/// The corpus is a checked-in file. The suites catch a line going
/// missing, but only for the selectors they use; this guards the file
/// as a whole against being emptied or truncated by a bad edit.
#[test]
fn corpus_is_populated() {
    let count = selectors().count();
    assert!(count >= 450, "corpus has shrunk to {count} selectors");
    assert!(
        corpus::contains("e:has(> .foo)"),
        "corpus is missing the README's examples"
    );
}

/// A line is added to the corpus by pasting what a sync failure printed,
/// so guard against the same selector being pasted twice.
#[test]
fn the_corpus_has_no_duplicate_lines() {
    let mut seen = std::collections::HashSet::new();
    let dupes: Vec<_> = selectors()
        .filter(|css| !css.is_empty() && !seen.insert(*css))
        .collect();
    assert!(dupes.is_empty(), "corpus repeats {dupes:?}");
}

/// Harness self-check: the membership test the suites assert against
/// must actually reject a selector the corpus does not hold, or the
/// sync check proves nothing.
#[test]
fn corpus_membership_rejects_an_absent_selector() {
    assert!(!corpus::contains("e:definitely-not-in-the-corpus"));
}

/// Harness self-check: sxd-xpath's parser must actually reject a
/// malformed expression, or the test above proves nothing.
#[test]
fn the_xpath_parser_rejects_malformed_input() {
    let factory = Factory::new();
    for bad in ["a[", "a[1", "a and", "'unterminated", "a//", "*[@]"] {
        assert!(
            !matches!(factory.build(bad), Ok(Some(_))),
            "sxd-xpath accepted {bad:?}, so it cannot vouch for our output"
        );
    }
}
