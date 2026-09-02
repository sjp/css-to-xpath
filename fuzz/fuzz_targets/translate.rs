//! Feed arbitrary text to all three translator modes.
//!
//! The crate invests heavily in bounding pathological input —
//! `MAX_NESTING_DEPTH`, `MAX_NTH_OF_DEPTH`, `MAX_NTH_OF_BYTES` and the
//! pre-parse `scan` — and this is what checks those bounds hold against
//! input nobody thought of. Two properties:
//!
//! 1. no panic, and no stack overflow on the 1 MiB stack the depth
//!    limit is sized for (hence the worker thread: libFuzzer's own
//!    thread is larger, which would hide a limit set too high);
//! 2. output stays proportionate to input.

#![no_main]

use css_to_xpath::{Mode, Translator};
use libfuzzer_sys::fuzz_target;

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
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        for prefix in ["", "descendant-or-self::"] {
            if let Ok(xpath) = Translator::new(mode).css_to_xpath(css, prefix) {
                let limit = SLACK + LINEAR.saturating_mul(css.len());
                assert!(
                    xpath.len() <= limit,
                    "output grew to {} bytes from {} bytes of input (limit {limit})",
                    xpath.len(),
                    css.len(),
                );
            }
        }
    }
}
