//! The nth-child arithmetic and the structural pseudo-classes Servo folds
//! into `NthSelectorData`.
//!
//! Servo parses `:first-child` as `nth-child` data with `(a, b) = (0, 1)`,
//! `:last-child` as `nth-last-child(0n+1)`, and so on. That collapse is
//! lossless for translation: a dedicated `:first-child` translation would
//! produce byte-identical output to the general an+b form on the same
//! `(a, b)` (e.g. both give `not(preceding-sibling::*[1])`). Only
//! `:only-child`/`:only-of-type` need their own translation.
//!
//! # Positional tests, not `count()`
//!
//! Every bound on the number of siblings is written as a positional
//! predicate — see [`sibling_count_is`]. `count()` materialises the whole
//! axis, so selecting over *n* siblings costs O(n²); `axis::T[k]` lets the
//! engine stop at the *k*-th node. Only the `mod a` congruence of a
//! repeating series still has to count.

use selectors::parser::{NthSelectorData, NthType, Selector};

use super::Translator;
use super::error::Error;
use super::xpath_expr::{Condition, XPathExpr};
use crate::parser::CssToXpathImpl;

/// The maximum `An+B of S` nesting depth accepted.
///
/// XPath 1.0 has no variables, so `S` must be written out more than once:
/// to filter the siblings being tested — twice when a test bounds the
/// sibling count from both sides, as `:nth-child(2 of S)` or
/// `:nth-child(2n+3 of S)` does — and once more to constrain the element
/// being matched. An `of S` list nested inside another therefore appears
/// in every copy, and the output doubles or triples per level — a
/// ~500-byte selector nesting 30 deep asks for far more memory than
/// exists. The duplication is inherent, so only a depth limit can bound
/// it, with [`MAX_NTH_OF_BYTES`] behind it for the arguments the depth
/// alone does not keep small. Nothing hand-written nests `of S` at all.
///
/// This is far below [`MAX_NESTING_DEPTH`](crate::MAX_NESTING_DEPTH), which bounds
/// *recursion* rather than output size and so can afford to be generous.
pub const MAX_NTH_OF_DEPTH: usize = 8;

/// The maximum size of one `of S` translation, a last line of defence
/// behind [`MAX_NTH_OF_DEPTH`](crate::MAX_NTH_OF_DEPTH): the duplication is bounded by the depth
/// limit, but the argument it copies is bounded only by the length of
/// the selector, so cap the product too. Checked per nesting level, which
/// caps the largest string ever built at roughly three times this.
pub const MAX_NTH_OF_BYTES: usize = 1 << 20;

/// A Level 4 `of S` argument list, carried together with how many other
/// such lists it is nested inside — the two are only ever meaningful
/// together, since the depth exists to bound this list's duplication.
#[derive(Clone, Copy)]
struct OfList<'a> {
    selectors: &'a [Selector<CssToXpathImpl>],
    depth: usize,
}

impl Translator {
    /// Route one `NthSelectorData` (with Servo's pre-parsed `(a, b)`) to
    /// the matching translation. `selector_list` carries the Level 4
    /// `of S` arguments when present (`Component::NthOf`).
    pub(crate) fn apply_nth(
        &self,
        xpath: &mut XPathExpr,
        data: &NthSelectorData,
        selector_list: Option<&[Selector<CssToXpathImpl>]>,
        of_depth: usize,
    ) -> Result<(), Error> {
        let a = data.an_plus_b.0;
        let b = data.an_plus_b.1;
        let of = selector_list.map(|selectors| OfList {
            selectors,
            depth: of_depth,
        });
        match data.ty {
            // :only-child — sibling tests rather than
            // count(parent::*/child::*) = 1, so the root element (whose
            // parent is the document node, not an element) matches, the
            // same way the equivalent :first-child:last-child does.
            NthType::OnlyChild => {
                xpath.add_condition(&format!(
                    "{} and {}",
                    sibling_count_is("preceding-sibling::*", Bound::Exactly, 0),
                    sibling_count_is("following-sibling::*", Bound::Exactly, 0),
                ));
                Ok(())
            }
            // :only-of-type
            NthType::OnlyOfType => {
                let nodetest = xpath.same_type_nodetest().ok_or_else(|| {
                    Error::unsupported("`:only-of-type` on the universal selector `*`")
                })?;
                xpath.add_condition(&format!(
                    "{} and {}",
                    sibling_count_is(&format!("preceding-sibling::{nodetest}"), Bound::Exactly, 0),
                    sibling_count_is(&format!("following-sibling::{nodetest}"), Bound::Exactly, 0),
                ));
                Ok(())
            }
            // :first-child / :last-child / :nth-child() / :nth-last-child()
            NthType::Child | NthType::LastChild => self.xpath_nth_child(
                xpath,
                a,
                b,
                /* last = */ data.ty == NthType::LastChild,
                /* nodetest = */ "*",
                of,
            ),
            // :first-of-type / :last-of-type / :nth-of-type() /
            // :nth-last-of-type() — none are implemented on the universal
            // selector `*`.
            NthType::OfType | NthType::LastOfType => {
                let nodetest = xpath.same_type_nodetest().ok_or_else(|| {
                    Error::unsupported("an of-type pseudo-class on the universal selector `*`")
                })?;
                self.xpath_nth_child(
                    xpath,
                    a,
                    b,
                    /* last = */ data.ty == NthType::LastOfType,
                    &nodetest,
                    of,
                )
            }
        }
    }

    /// The general an+b translation, derived from
    /// https://www.w3.org/TR/selectors-4/#structural-pseudos.
    ///
    /// `nodetest` selects which siblings are counted: `*` for the child
    /// pseudos, the same-type node test for the of-type pseudos.
    fn xpath_nth_child(
        &self,
        xpath: &mut XPathExpr,
        a: i32,
        b: i32,
        last: bool,
        nodetest: &str,
        of: Option<OfList<'_>>,
    ) -> Result<(), Error> {
        // i64 throughout: `-(b-1)` / `abs(a)` must not overflow for
        // extreme i32 inputs.
        let a = i64::from(a);
        let b = i64::from(b);

        // work with b-1 instead
        let b_min_1 = b - 1;

        // CSS Level 4: when a selector list is provided, the current
        // element must match it too. The same OR-joined condition is
        // appended in every branch *and* rendered into the sibling
        // predicate below, so each level of `of S` nesting doubles or
        // triples the output: both limits guard that growth.
        // A trivially-true list (it contains a universal argument)
        // constrains nothing, like a plain :nth-child.
        let current_element_check = match of {
            Some(of) => {
                if of.depth >= MAX_NTH_OF_DEPTH {
                    return Err(Error::unsupported(format!(
                        "`An+B of S` selector lists nested more than \
                         {MAX_NTH_OF_DEPTH} levels deep"
                    )));
                }
                let check = self
                    .arg_conditions(of.selectors, ":nth-child(... of S)", of.depth + 1)?
                    .and_then(|conditions| Condition::join_or(&conditions));
                if check
                    .as_ref()
                    .is_some_and(|c| c.expr.len() > MAX_NTH_OF_BYTES)
                {
                    return Err(Error::unsupported(format!(
                        "an `An+B of S` selector list translating to more than \
                         {MAX_NTH_OF_BYTES} bytes"
                    )));
                }
                check
            }
            None => None,
        };

        // early-exit condition 1:
        // ~~~~~~~~~~~~~~~~~~~~~~~
        // for a == 1, nth-*(an+b) means n+b-1 siblings before/after, and
        // since n is a non-negative integer, if b-1<=0 there is always an
        // "n" matching any number of siblings (maybe none)
        if a == 1 && b_min_1 <= 0 {
            if let Some(check) = current_element_check {
                xpath.push_condition(check);
            }
            return Ok(());
        }
        // early-exit condition 2:
        // ~~~~~~~~~~~~~~~~~~~~~~~
        // an+b-1 siblings with (b-1)<0 needs a>0 to reach zero, so for
        // a<=0 nothing can match. Writing it as `0` rather than letting
        // the a==0 branch below ask for a negative sibling count says so
        // plainly.
        if a <= 0 && b_min_1 < 0 {
            xpath.add_condition("0");
            if let Some(check) = current_element_check {
                xpath.push_condition(check);
            }
            return Ok(());
        }

        // The predicate filtering counted siblings (CSS Level 4 `of S`) —
        // the same OR-joined conditions as the current-element check.
        let selector_predicate = match current_element_check {
            Some(ref check) => format!("[{}]", check.expr),
            None => String::new(),
        };

        // the siblings before or after the element
        let axis = if last { "following" } else { "preceding" };
        let siblings = format!("{axis}-sibling::{nodetest}{selector_predicate}");

        // special case of fixed position: nth-*(0n+b)
        if a == 0 {
            xpath.add_condition(&sibling_count_is(&siblings, Bound::Exactly, b_min_1));
            if let Some(check) = current_element_check {
                xpath.push_condition(check);
            }
            return Ok(());
        }

        let mut expr: Vec<String> = Vec::new();

        if a > 0 {
            // siblings count, an+b-1, is always >= 0, so if a>0 and
            // (b-1)<=0 an "n" exists to satisfy this; the predicate is
            // only interesting if (b-1)>0
            if b_min_1 > 0 {
                expr.push(sibling_count_is(&siblings, Bound::AtLeast, b_min_1));
            }
        } else {
            // a<0 with (b-1)<0 was the early exit above; otherwise:
            expr.push(sibling_count_is(&siblings, Bound::AtMost, b_min_1));
        }

        // operations modulo 1 or -1 are simpler: the >=/<= test above
        // already covers them
        if a.abs() != 1 {
            // count(***-sibling::***) - (b-1) = 0 (mod a) — the one test
            // with no positional equivalent
            let mut left = format!("count({siblings})");

            // apply "modulo a" on the 2nd term, -(b-1), to simplify things
            // like "(... +6) % -3", and also make it positive with |a|
            // (`rem_euclid`)
            let b_neg = (-b_min_1).rem_euclid(a.abs());

            if b_neg != 0 {
                left = format!("({left} + {b_neg})");
            }

            expr.push(format!("{left} mod {a} = 0"));
        }

        if !expr.is_empty() {
            xpath.add_condition(&expr.join(" and "));
        }

        if let Some(check) = current_element_check {
            xpath.push_condition(check);
        }

        Ok(())
    }
}

/// How [`sibling_count_is`] bounds the number of siblings.
#[derive(Clone, Copy)]
enum Bound {
    Exactly,
    AtLeast,
    AtMost,
}

/// A test that the node-set `siblings` — a reverse or forward sibling
/// axis step, predicates and all — has `Exactly`, `AtLeast` or `AtMost`
/// `k` members, written with positional predicates rather than `count()`.
///
/// `siblings[k]` is non-empty exactly when there are at least `k`
/// siblings: on either sibling axis, position counts outwards from the
/// context node. A trailing `[S]` in `siblings` filters before the
/// position applies, so `[S][k]` is the `k`-th sibling matching `S`.
///
/// The `[1]` in the zero case is load-bearing: libxml2 takes a
/// pathological path on a bare `not(preceding-sibling::*)` — minutes for a
/// list `count()` answers in seconds — which the positional form avoids.
fn sibling_count_is(siblings: &str, bound: Bound, k: i64) -> String {
    debug_assert!(k >= 0, "a sibling count is never negative");
    match bound {
        Bound::Exactly if k == 0 => format!("not({siblings}[1])"),
        Bound::Exactly => format!("{siblings}[{k}] and not({siblings}[{}])", k + 1),
        Bound::AtLeast => {
            debug_assert!(k > 0, "at least zero siblings is no test at all");
            format!("{siblings}[{k}]")
        }
        Bound::AtMost => format!("not({siblings}[{}])", k + 1),
    }
}
