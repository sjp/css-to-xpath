//! Property test for the An+B arithmetic.
//!
//! `xpath_nth_child` is arithmetic with several early exits and a
//! `rem_euclid` normalisation, and it emits three different predicate
//! shapes depending on the signs of `a` and `b`. Rather than pin the
//! strings, this generates `An+B`, builds a document with a known number
//! of siblings, evaluates the translated XPath, and compares the
//! selected positions against the definition: position `p` matches when
//! `p == a*n + b` for some integer `n >= 0`.

mod common;

use common::Fixture;
use css_to_xpath::Mode;
use proptest::prelude::*;

/// The reference reading of An+B, straight from the definition.
///
/// Computed in `i64` so that `i32::MIN` operands do not overflow the
/// *reference*; the translator is the thing under test here.
fn matches(a: i32, b: i32, position: i32) -> bool {
    let (a, b, position) = (i64::from(a), i64::from(b), i64::from(position));
    if a == 0 {
        return position == b;
    }
    // position == a*n + b for some integer n >= 0.
    let offset = position - b;
    if offset == 0 {
        return true;
    }
    offset.signum() == a.signum() && offset % a == 0
}

/// `An+B` in CSS syntax. `b` is always written with an explicit sign so
/// that the two halves cannot run together.
fn an_plus_b(a: i32, b: i32) -> String {
    if b < 0 {
        format!("{a}n-{}", b.unsigned_abs())
    } else {
        format!("{a}n+{b}")
    }
}

/// `<r>` holding `count` `<c>` children, each id'd by its 1-based
/// position.
fn siblings(count: i32) -> Fixture {
    let mut xml = String::from("<r id=\"r\">");
    for position in 1..=count {
        xml.push_str(&format!("<c id=\"{position}\"/>"));
    }
    xml.push_str("</r>");
    Fixture::new(&xml, &[])
}

fn expected(count: i32, a: i32, b: i32, from_end: bool) -> Vec<String> {
    (1..=count)
        .filter(|&position| {
            let index = if from_end {
                count - position + 1
            } else {
                position
            };
            matches(a, b, index)
        })
        .map(|position| position.to_string())
        .collect()
}

proptest! {
    #[test]
    fn nth_child_selects_exactly_the_positions_an_plus_b_describes(
        a in -12i32..=12,
        b in -12i32..=24,
        count in 0i32..=24,
    ) {
        let fixture = siblings(count);
        let css = format!("c:nth-child({})", an_plus_b(a, b));
        prop_assert_eq!(
            fixture.select(&css, Mode::Generic),
            expected(count, a, b, false),
            "{}", css
        );
    }

    #[test]
    fn nth_last_child_counts_from_the_end(
        a in -12i32..=12,
        b in -12i32..=24,
        count in 0i32..=24,
    ) {
        let fixture = siblings(count);
        let css = format!("c:nth-last-child({})", an_plus_b(a, b));
        prop_assert_eq!(
            fixture.select(&css, Mode::Generic),
            expected(count, a, b, true),
            "{}", css
        );
    }
}

proptest! {
    /// `An+B of S` counts positions among the siblings matching `S`,
    /// not among all siblings.
    #[test]
    fn nth_child_of_s_counts_within_the_matching_siblings(
        a in -8i32..=8,
        b in -8i32..=16,
        count in 0i32..=20,
    ) {
        // Every third sibling is left out of the `.m` set, so the two
        // numberings diverge.
        let marked = |position: i32| position % 3 != 0;

        let mut xml = String::from("<r id=\"r\">");
        for position in 1..=count {
            let class = if marked(position) { " class=\"m\"" } else { "" };
            xml.push_str(&format!("<c id=\"{position}\"{class}/>"));
        }
        xml.push_str("</r>");
        let fixture = Fixture::new(&xml, &[]);

        let mut want = Vec::new();
        let mut index = 0;
        for position in 1..=count {
            if marked(position) {
                index += 1;
                if matches(a, b, index) {
                    want.push(position.to_string());
                }
            }
        }

        let css = format!("c:nth-child({} of .m)", an_plus_b(a, b));
        prop_assert_eq!(fixture.select(&css, Mode::Generic), want, "{}", css);
    }
}

/// The extremes the generated range cannot reach: `a` and `b` at i32's
/// limits, where the arithmetic could overflow.
#[test]
fn nth_child_at_i32_extremes() {
    let fixture = siblings(8);
    for (a, b) in [
        (i32::MAX, 1),
        (i32::MIN, 1),
        (1, i32::MAX),
        (1, i32::MIN),
        (i32::MAX, i32::MAX),
        (i32::MIN, i32::MIN),
        (0, i32::MAX),
        (0, i32::MIN),
    ] {
        let css = format!("c:nth-child({})", an_plus_b(a, b));
        assert_eq!(
            fixture.select(&css, Mode::Generic),
            expected(8, a, b, false),
            "{css}"
        );
    }
}

/// The keyword forms, which the parser expands to 2n+1 and 2n+0.
#[test]
fn odd_and_even_keywords() {
    let fixture = siblings(9);
    assert_eq!(
        fixture.select("c:nth-child(odd)", Mode::Generic),
        expected(9, 2, 1, false)
    );
    assert_eq!(
        fixture.select("c:nth-child(even)", Mode::Generic),
        expected(9, 2, 0, false)
    );
    assert_eq!(
        fixture.select("c:nth-last-child(odd)", Mode::Generic),
        expected(9, 2, 1, true)
    );
}
