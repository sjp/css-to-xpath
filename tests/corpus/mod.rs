//! The shared selector corpus, and the membership check that keeps it in
//! sync with the suites that feed it.
//!
//! `selectors.txt` is the crate's list of "selectors we know about". The
//! XPath-validity oracle translates every line in all three modes, with
//! and without a path prefix, and `fuzz/seed-corpus.sh` turns every line
//! into a fuzzer seed — so a selector that is exercised by a test but
//! missing here is a shape the oracle and the fuzzer never see.
//!
//! Keeping the file correct by hand does not scale: nothing stops a new
//! row in `tests/nth.rs` from never reaching it. So the suites record
//! instead of maintain — [`contains`] is asserted for every selector the
//! string-pinning suites pin and every selector `tests/semantics.rs`
//! evaluates, and the failure names the lines to add.
//!
//! Pulled in by both `tests/cases/mod.rs` and `tests/common/mod.rs` with
//! `#[path]`, so any single test binary uses only part of it; hence the
//! blanket `dead_code` allow.
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::OnceLock;

/// The path to quote when a selector is missing, as the reader would
/// type it from the repository root.
pub(crate) const PATH: &str = "tests/corpus/selectors.txt";

/// Every selector in the corpus, in file order.
///
/// One selector per line, stored raw — so a selector containing a line
/// break cannot be represented here. That is the only gap, and the fuzz
/// target covers arbitrary bytes anyway.
pub(crate) fn selectors() -> impl Iterator<Item = &'static str> {
    include_str!("selectors.txt").lines()
}

/// Whether `css` is one of the corpus's lines.
pub(crate) fn contains(css: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| selectors().collect()).contains(css)
}
