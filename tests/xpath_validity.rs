//! Every expression this crate emits must be syntactically valid XPath
//! 1.0.
//!
//! The unit tests in `src/lib.rs` pin output strings, which pins the
//! translation but says nothing about whether the string is an XPath at
//! all: an unbalanced bracket, a bad literal or a precedence mistake
//! would sail through. Here every selector in the shared corpus (which
//! is those same unit-test selectors) is translated in all three modes,
//! with and without a path prefix, and the result is handed to
//! sxd-xpath's parser.

mod common;

use common::{MODES, corpus, mode_name};
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

    for css in corpus() {
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

/// The corpus is a checked-in file; guard against it being emptied or
/// truncated by a bad edit.
#[test]
fn corpus_is_populated() {
    let count = corpus().count();
    assert!(count >= 450, "corpus has shrunk to {count} selectors");
    assert!(
        corpus().any(|s| s == "e:has(> .foo)"),
        "corpus is missing the README's examples"
    );
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
