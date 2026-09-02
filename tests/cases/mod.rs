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

use css_to_xpath::{Error, Mode, Translator};

/// A translator for one [`Mode`] that accumulates mismatches instead of
/// panicking at the first one.
///
/// The accumulated failures are reported when the checker is dropped, so
/// a test needs no closing call; a checker that never sees a mismatch is
/// silent. Anything the translator itself can do is still reachable
/// through [`Cases::css_to_xpath`], for the cases whose expectation is
/// an error rather than an output string.
pub struct Cases {
    translator: Translator,
    prefix: &'static str,
    failures: String,
    failed: usize,
}

impl Cases {
    /// A checker translating with no path prefix.
    pub fn new(mode: Mode) -> Self {
        Self::with_prefix(mode, "")
    }

    /// A checker translating with `prefix` prepended to each branch.
    pub fn with_prefix(mode: Mode, prefix: &'static str) -> Self {
        Cases {
            translator: Translator::new(mode),
            prefix,
            failures: String::new(),
            failed: 0,
        }
    }

    /// The mode this checker translates in.
    pub fn mode(&self) -> Mode {
        self.translator.mode()
    }

    /// Translate `css`, exactly as [`Translator::css_to_xpath`] would.
    ///
    /// For expectations this checker cannot express: an error, or a
    /// property of the output other than equality.
    pub fn css_to_xpath(&self, css: &str, prefix: &str) -> Result<String, Error> {
        self.translator.css_to_xpath(css, prefix)
    }

    /// Translate `css`, panicking with the selector if it fails.
    ///
    /// For an expectation stated as a property of the output — its
    /// length, or equality with another selector's translation.
    pub fn xpath(&self, css: &str) -> String {
        self.translator
            .css_to_xpath(css, self.prefix)
            .unwrap_or_else(|e| panic!("{css:?} failed to translate: {e}"))
    }

    /// Translate `css` and record a failure unless the result is
    /// `expected`. A selector that fails to translate at all is a
    /// failure too, reported with its error.
    pub fn check(&mut self, css: &str, expected: impl AsRef<str>) {
        let expected = expected.as_ref();
        match self.translator.css_to_xpath(css, self.prefix) {
            Ok(got) if got == expected => {}
            Ok(got) => self.fail(css, &format!("expected {expected:?}\n    got      {got:?}")),
            Err(e) => self.fail(css, &format!("expected {expected:?}\n    got error {e}")),
        }
    }

    /// [`Cases::check`] over a table of `(selector, xpath)` pairs.
    pub fn all(&mut self, pairs: &[(&str, &str)]) {
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
        assert!(
            self.failures.is_empty() || std::thread::panicking(),
            "{} case(s) failed:{}",
            self.failed,
            self.failures
        );
    }
}
