//! The translation-case checker shared by the string-pinning suites.
//!
//! Each suite pins the exact XPath a family of selectors translates to.
//! A bare `assert_eq!` is a poor fit for that: it prints the two XPath
//! strings but not the selector they came from, and it aborts the test
//! function on the first mismatch, hiding every later regression in the
//! same family. [`Cases`] does neither — it names the offending selector
//! and reports every mismatch at once.
//!
//! Each test binary pulls this in with `mod cases;`, so any single
//! binary uses only part of it; hence the blanket `dead_code` allow.
#![allow(dead_code)]

/// The shared selector corpus, which every pinned selector must be in.
#[path = "../corpus/mod.rs"]
mod corpus;

use css_to_xpath::{Error, Mode, Translator};

/// A translator for one [`Mode`] that accumulates mismatches instead of
/// panicking at the first one.
///
/// The accumulated failures are reported when the checker is dropped, so
/// a test needs no closing call; a checker that never sees a mismatch is
/// silent. Anything the translator itself can do is still reachable
/// through [`Cases::css_to_xpath`], for the cases whose expectation is
/// an error rather than an output string.
///
/// Dropping also reports any pinned selector missing from the shared
/// corpus, which is what keeps `tests/corpus/selectors.txt` — the input
/// to the XPath-validity oracle and to the fuzzer's seeds — in step with
/// the suites instead of relying on the author to remember it.
pub(crate) struct Cases {
    translator: Translator,
    prefix: &'static str,
    failures: String,
    failed: usize,
    uncorpused: Vec<String>,
}

impl Cases {
    /// A checker translating with no path prefix.
    pub(crate) fn new(mode: Mode) -> Self {
        Self::with_prefix(mode, "")
    }

    /// A checker translating with `prefix` prepended to each branch.
    pub(crate) fn with_prefix(mode: Mode, prefix: &'static str) -> Self {
        Self::with_translator(Translator::new(mode), prefix)
    }

    /// A checker translating with a configured `translator` — for what a
    /// bare [`Mode`] cannot express, namely a default namespace prefix.
    pub(crate) fn with_translator(translator: Translator, prefix: &'static str) -> Self {
        Cases {
            translator,
            prefix,
            failures: String::new(),
            failed: 0,
            uncorpused: Vec::new(),
        }
    }

    /// The mode this checker translates in.
    pub(crate) fn mode(&self) -> Mode {
        self.translator.mode()
    }

    /// Translate `css`, exactly as [`Translator::css_to_xpath`] would.
    ///
    /// For expectations this checker cannot express: an error, or a
    /// property of the output other than equality.
    pub(crate) fn css_to_xpath(&self, css: &str, prefix: &str) -> Result<String, Error> {
        self.translator.css_to_xpath(css, prefix)
    }

    /// Translate `css`, panicking with the selector if it fails.
    ///
    /// For an expectation stated as a property of the output — its
    /// length, or equality with another selector's translation.
    pub(crate) fn xpath(&self, css: &str) -> String {
        self.translator
            .css_to_xpath(css, self.prefix)
            .unwrap_or_else(|e| panic!("{css:?} failed to translate: {e}"))
    }

    /// Translate `css` and record a failure unless the result is
    /// `expected`. A selector that fails to translate at all is a
    /// failure too, reported with its error.
    pub(crate) fn check(&mut self, css: &str, expected: impl AsRef<str>) {
        let expected = expected.as_ref();
        if !corpus::contains(css) && !self.uncorpused.iter().any(|s| s == css) {
            self.uncorpused.push(css.to_owned());
        }
        match self.translator.css_to_xpath(css, self.prefix) {
            Ok(got) if got == expected => {}
            Ok(got) => self.fail(css, &format!("expected {expected:?}\n    got      {got:?}")),
            Err(e) => self.fail(css, &format!("expected {expected:?}\n    got error {e}")),
        }
    }

    /// [`Cases::check`] over a table of `(selector, xpath)` pairs.
    pub(crate) fn all(&mut self, pairs: &[(&str, &str)]) {
        for (css, expected) in pairs {
            self.check(css, expected);
        }
    }

    fn fail(&mut self, css: &str, detail: &str) {
        self.failed += 1;
        let mode = self.mode();
        self.failures
            .push_str(&format!("\n  {css:?} in {mode:?}\n    {detail}"));
    }
}

/// Reporting on drop keeps the call sites free of bookkeeping. A panic
/// while the thread is already unwinding would abort the process, so a
/// test that failed some other way reports that failure alone.
impl Drop for Cases {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }

        let mut report = String::new();
        if !self.failures.is_empty() {
            report.push_str(&format!("{} case(s) failed:{}", self.failed, self.failures));
        }
        if !self.uncorpused.is_empty() {
            report.push_str(&format!(
                "\n{} pinned selector(s) missing from {}; add these lines:\n{}",
                self.uncorpused.len(),
                corpus::PATH,
                self.uncorpused.join("\n"),
            ));
        }
        assert!(report.is_empty(), "{}", report.trim_start_matches('\n'));
    }
}
