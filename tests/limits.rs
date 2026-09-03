//! The bounds that keep pathological input from exhausting the stack
//! or the heap: nesting depth, `of S` blow-up, and chain length.

mod cases;
use cases::Cases;
use css_to_xpath::{Mode, Translator};

/// Nesting depth is bounded before Servo is entered, so neither the
/// parser, the translator, nor dropping the selector tree can recurse
/// far enough to overflow the stack — an overflow aborts the process
/// outright, which no caller can catch. The whole test runs on a
/// thread with the 1 MiB stack the limit is sized against, which is
/// the *smallest* stack the crate expects (a Windows main thread, a
/// wasm32 module, a thread pool's worker) rather than the 2 MB Rust
/// gives a spawned thread by default. Testing against the generous
/// figure would hide a limit set too high for the tight one, so a
/// regression fails the build instead of tearing the test runner
/// down — or, worse, only the caller's.
#[test]
fn nesting_depth_is_bounded() {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let t = Translator::new(Mode::Generic);
            let nest = |open: &str, n: usize| format!("{}a{}", open.repeat(n), ")".repeat(n));
            // The position reported is the 33rd `(` — the first one past
            // the limit — so it does not move with however much deeper
            // the rest of the selector goes: 33 levels and 10 000 levels
            // of the same construct are the same error.
            let too_deep = |offset: usize| css_to_xpath::Error::Unsupported {
                construct: "functional pseudo-classes nested more than 32 levels deep".to_owned(),
                offset: Some(offset),
            };

            for open in [":not(", ":is(", ":where(", ":matches("] {
                let over_limit = too_deep(33 * open.len() - 1);
                assert!(t.css_to_xpath(&nest(open, 32), "").is_ok());
                assert_eq!(t.css_to_xpath(&nest(open, 33), "").unwrap_err(), over_limit);
                // Well past the depth that used to abort the process.
                assert_eq!(
                    t.css_to_xpath(&nest(open, 10_000), "").unwrap_err(),
                    over_limit
                );
            }

            // Parens inside strings, escapes, and comments are not
            // nesting, exactly as the `||` scan treats pipes.
            let parens = "(".repeat(1000);
            assert!(t.css_to_xpath(&format!("[foo=\"{parens}\"]"), "").is_ok());
            assert!(t.css_to_xpath(&format!("a /* {parens} */ b"), "").is_ok());
            assert!(t.css_to_xpath(&"a\\(".repeat(1000), "").is_ok());

            // Depth is the limit, not length: an argument chain adds
            // no stack frames, so a chain far longer than the depth
            // limit still translates.
            let chain = vec!["a"; 20_000].join(" > ");
            assert!(t.css_to_xpath(&format!(":is({chain})"), "").is_ok());
            assert!(t.css_to_xpath(&format!("b:has({chain})"), "").is_ok());
            assert!(t.css_to_xpath(&chain, "").is_ok());
        })
        .unwrap()
        .join()
        .unwrap();
}

/// Length is linear, not quadratic: every combinator appends to the
/// accumulated path instead of re-rendering it, so a chain far longer
/// than anything a person would write still translates promptly. The
/// assertions are on the output, not the clock — a regression to
/// re-rendering shows up as the test taking minutes.
#[test]
fn long_chains_translate() {
    let t = Translator::new(Mode::Generic);
    let chain = vec!["a"; 100_000].join(" > ");
    let xpath = t.css_to_xpath(&chain, "//").unwrap();
    assert_eq!(xpath, format!("//{}", vec!["a"; 100_000].join("/")));

    // The same chain as an argument nests one existence test per
    // compound, which the wrapping must likewise build in one pass.
    let inner = t.css_to_xpath(&format!("b:is({chain})"), "//").unwrap();
    assert_eq!(
        inner,
        format!(
            "//b[{}self::a{}]",
            "self::a and parent::*[".repeat(99_999),
            "]".repeat(99_999)
        )
    );
}

/// The size of an `of S` translation is bounded. XPath 1.0 has no
/// variables, so `S` is written out twice — into the sibling
/// predicate and into the current-element check — and a nested `of S`
/// lands in both copies, doubling the output per level. Only a limit
/// can fix that, so both the nesting depth and the size of one level
/// are capped.
#[test]
fn nth_child_of_nesting_is_bounded() {
    let mut t = Cases::new(Mode::Generic);
    let nest = |n: usize| format!("{}a{}", ":nth-child(2 of ".repeat(n), ")".repeat(n));
    // Unlike the parenthesis limit, this one is reached during
    // translation, where Servo's components carry no source offsets:
    // the error names the construct but no position.
    let too_deep = css_to_xpath::Error::Unsupported {
        construct: "`An+B of S` selector lists nested more than 8 levels deep".to_owned(),
        offset: None,
    };

    // Two levels, in full: `a` appears four times, not twice.
    t.check(
        &nest(2),
        "*[count(preceding-sibling::*[count(preceding-sibling::*[self::a]) = 1 \
          and self::a]) = 1 \
          and count(preceding-sibling::*[self::a]) = 1 and self::a]",
    );
    // The doubling itself: each level is a little over twice the last.
    assert_eq!(t.xpath(&nest(4)).len(), 685);
    assert_eq!(t.xpath(&nest(8)).len(), 11_485);

    // Past the limit it is an error, and a cheap one: the depth is
    // checked before descending, so nothing exponential is built.
    assert_eq!(t.css_to_xpath(&nest(9), "").unwrap_err(), too_deep);
    assert_eq!(t.css_to_xpath(&nest(24), "").unwrap_err(), too_deep);
    // Further out, the pre-parse paren scan gets there first: each
    // `of S` level spends a parenthesis, so past `MAX_NESTING_DEPTH`
    // of them the generic nesting error is what a caller sees. Both
    // reject the same selectors; only the wording differs.
    assert_eq!(
        t.css_to_xpath(&nest(33), "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "functional pseudo-classes nested more than 32 levels deep".to_owned(),
            // The 33rd `(`: one per `:nth-child(2 of ` level, whose 16
            // bytes put it at offset 10.
            offset: Some(32 * 16 + 10),
        }
    );
    // The selector is 154 bytes, so its quote is elided at 120
    // with a `…`: every message is bounded, not just the
    // caret-bearing ones.
    assert_eq!(
        too_deep.message(&nest(9)),
        format!(
            "The CSS selector \"{}\u{2026}\" uses `An+B of S` selector lists nested \
             more than 8 levels deep, which this translator does not support",
            &nest(9)[..120]
        )
    );

    // Depth counts `of S` lists wherever they sit, including inside
    // another functional pseudo-class.
    let laundered = |n: usize| format!("{}a{}", ":nth-child(2 of :is(".repeat(n), "))".repeat(n));
    assert!(t.css_to_xpath(&laundered(8), "").is_ok());
    assert_eq!(
        t.css_to_xpath(&laundered(9), "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "`An+B of S` selector lists nested more than 8 levels deep".to_owned(),
            offset: None,
        }
    );

    // The depth limit bounds the doubling, not what is doubled, so a
    // large argument at full depth is capped by size as well.
    let big = |k: usize| {
        let args: Vec<String> = (0..k).map(|i| format!("a{i}")).collect();
        format!(
            "{}:is({}){}",
            ":nth-child(2 of ".repeat(8),
            args.join(","),
            ")".repeat(8)
        )
    };
    assert!(t.css_to_xpath(&big(200), "").is_ok());
    assert_eq!(
        t.css_to_xpath(&big(800), "").unwrap_err(),
        css_to_xpath::Error::Unsupported {
            construct: "an `An+B of S` selector list translating to more than 1048576 bytes"
                .to_owned(),
            offset: None,
        }
    );
}
