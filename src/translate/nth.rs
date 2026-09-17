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
//! predicate — see [`push_sibling_count`]. `count()` materialises the whole
//! axis, so selecting over *n* siblings costs O(n²); `axis::T[k]` lets the
//! engine stop at the *k*-th node. Only the `mod a` congruence of a
//! repeating series still has to count.

use std::fmt::{self, Write as _};

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
                let mut cond = String::new();
                push_sibling_count(&mut cond, "preceding-sibling::*", Bound::Exactly, 0);
                cond.push_str(" and ");
                push_sibling_count(&mut cond, "following-sibling::*", Bound::Exactly, 0);
                xpath.add_condition(cond);
                Ok(())
            }
            // :only-of-type
            NthType::OnlyOfType => {
                let nodetest = xpath.same_type_nodetest().ok_or_else(|| {
                    Error::unsupported("`:only-of-type` on the universal selector `*`")
                })?;
                let mut cond = String::new();
                for (i, last) in [false, true].into_iter().enumerate() {
                    if i > 0 {
                        cond.push_str(" and ");
                    }
                    let siblings = Siblings::new(last, &nodetest, None);
                    push_sibling_count(&mut cond, siblings, Bound::Exactly, 0);
                }
                xpath.add_condition(cond);
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
                    .and_then(Condition::join_or);
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
            if let Some(check) = current_element_check {
                xpath.push_condition(check);
            }
            xpath.add_condition("0");
            return Ok(());
        }

        // The siblings before or after the element, filtered by `S` (CSS
        // Level 4 `of S`) — the same OR-joined conditions as the
        // current-element check. Every term below is written straight
        // into one buffer: `S` can be up to `MAX_NTH_OF_BYTES`, and any
        // intermediate string holding it multiplies what a nested `of S`
        // allocates.
        let siblings = Siblings::new(
            last,
            nodetest,
            current_element_check.as_ref().map(|c| c.expr.as_str()),
        );
        // At most two copies of `siblings` (an exact position, or a bound
        // and the congruence), plus the fixed text and up to three
        // integers around them.
        let mut cond = String::with_capacity(2 * siblings.len() + 96);

        // special case of fixed position: nth-*(0n+b)
        if a == 0 {
            push_sibling_count(&mut cond, siblings, Bound::Exactly, b_min_1);
        } else {
            if a > 0 {
                // siblings count, an+b-1, is always >= 0, so if a>0 and
                // (b-1)<=0 an "n" exists to satisfy this; the predicate is
                // only interesting if (b-1)>0. Nor is it if (b-1)<a: the
                // smallest count congruent to b-1 modulo a is then b-1
                // itself, so the `mod` test below already implies the
                // bound.
                if b_min_1 >= a {
                    push_sibling_count(&mut cond, siblings, Bound::AtLeast, b_min_1);
                }
            } else {
                // a<0 with (b-1)<0 was the early exit above; otherwise:
                push_sibling_count(&mut cond, siblings, Bound::AtMost, b_min_1);
            }

            // operations modulo 1 or -1 are simpler: the >=/<= test above
            // already covers them
            if a.abs() != 1 {
                if !cond.is_empty() {
                    cond.push_str(" and ");
                }
                // count(***-sibling::***) - (b-1) = 0 (mod a) — the one
                // test with no positional equivalent.
                //
                // Apply "modulo a" on the 2nd term, -(b-1), to simplify
                // things like "(... +6) % -3", and also make it positive
                // with |a| (`rem_euclid`).
                let b_neg = (-b_min_1).rem_euclid(a.abs());
                let written = if b_neg == 0 {
                    write!(cond, "count({siblings}) mod {a} = 0")
                } else {
                    write!(cond, "(count({siblings}) + {b_neg}) mod {a} = 0")
                };
                written.expect("writing to a String cannot fail");
            }
        }

        // The current-element check goes before the sibling test: `and`
        // evaluates left to right and stops at the first false operand,
        // so a candidate that does not match `S` skips the sibling walk.
        if let Some(check) = current_element_check {
            xpath.push_condition(check);
        }
        if !cond.is_empty() {
            xpath.add_condition(cond);
        }

        Ok(())
    }
}

/// How [`push_sibling_count`] bounds the number of siblings.
#[derive(Clone, Copy)]
enum Bound {
    Exactly,
    AtLeast,
    AtMost,
}

/// A sibling axis step, `{axis}-sibling::{nodetest}` with an optional
/// `[{filter}]`, written in place wherever it is formatted rather than
/// built once and copied.
#[derive(Clone, Copy)]
struct Siblings<'a> {
    axis: &'static str,
    nodetest: &'a str,
    filter: Option<&'a str>,
}

impl<'a> Siblings<'a> {
    /// The siblings after the element when `last`, else those before it.
    const fn new(last: bool, nodetest: &'a str, filter: Option<&'a str>) -> Self {
        let axis = if last {
            "following-sibling::"
        } else {
            "preceding-sibling::"
        };
        Self {
            axis,
            nodetest,
            filter,
        }
    }

    /// The formatted length in bytes.
    fn len(&self) -> usize {
        self.axis.len() + self.nodetest.len() + self.filter.map_or(0, |f| f.len() + 2)
    }
}

impl fmt::Display for Siblings<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.axis)?;
        f.write_str(self.nodetest)?;
        if let Some(filter) = self.filter {
            f.write_str("[")?;
            f.write_str(filter)?;
            f.write_str("]")?;
        }
        Ok(())
    }
}

/// Append to `out` a test that the node-set `siblings` — a reverse or
/// forward sibling axis step, predicates and all — has `Exactly`,
/// `AtLeast` or `AtMost` `k` members, written with positional predicates
/// rather than `count()`.
///
/// `siblings[k]` is non-empty exactly when there are at least `k`
/// siblings: on either sibling axis, position counts outwards from the
/// context node. A trailing `[S]` in `siblings` filters before the
/// position applies, so `[S][k]` is the `k`-th sibling matching `S`.
///
/// The `[1]` in the zero case is load-bearing: libxml2 takes a
/// pathological path on a bare `not(preceding-sibling::*)` — minutes for a
/// list `count()` answers in seconds — which the positional form avoids.
fn push_sibling_count(out: &mut String, siblings: impl fmt::Display, bound: Bound, k: i64) {
    debug_assert!(k >= 0, "a sibling count is never negative");
    let written = match bound {
        Bound::Exactly if k == 0 => write!(out, "not({siblings}[1])"),
        Bound::Exactly => write!(out, "{siblings}[{k}] and not({siblings}[{}])", k + 1),
        Bound::AtLeast => {
            debug_assert!(k > 0, "at least zero siblings is no test at all");
            write!(out, "{siblings}[{k}]")
        }
        Bound::AtMost => write!(out, "not({siblings}[{}])", k + 1),
    };
    written.expect("writing to a String cannot fail");
}
