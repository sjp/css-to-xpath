//! The `XPathExpr` builder and string helpers.
//!
//! Conditions are stored unparenthesized and parenthesized only at render
//! time, and only where XPath precedence requires it: an expression with a
//! top-level `or` (a `Condition` with `or_group` set) is wrapped when it
//! is conjoined with other conditions, since `and` binds tighter than
//! `or`. The exact output (like `e[@foo = 'bar']`) is load-bearing for
//! the crate's output contract and is pinned by tests.

use std::borrow::Cow;
use std::fmt;

/// Whether a *local* name can be used directly in an XPath name test (no
/// quoting needed).
///
/// Deliberately ASCII-only, which is conservative rather than exact: a
/// name that fails here folds into a `local-name()` or `name()`
/// comparison that means the same thing, so the only cost of rejecting a
/// name XPath would have accepted is a longer expression. A namespace
/// *prefix* has no such fallback and is tested against the real
/// `NCName` production instead; see [`super::ncname`].
pub(crate) fn is_safe_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// XPath 1.0 has no case-folding function, so every case-insensitive
/// comparison this crate emits is an ASCII fold through `translate()`:
/// the alphabet is written here once and shared by the `i` attribute
/// flag, HTML's legacy case-insensitive attributes, and the enumerated
/// `type` keyword the HTML pseudo-classes compare against. Only A-Z is
/// folded, matching CSS's and HTML's ASCII-only case-insensitivity.
pub(crate) fn ascii_lower(subject: &str) -> String {
    format!(
        "translate({subject}, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz')"
    )
}

/// Quote a string as an XPath literal, written wherever the result is
/// formatted.
///
/// XPath 1.0 literals have no escape syntax, so a string containing both
/// quote kinds cannot be written as one literal and has to be
/// `concat()`ed from several. Splitting it into *maximal* runs — each
/// run of apostrophes quoted with `"`, everything between them quoted
/// with `'` — keeps that fallback proportional to the number of
/// apostrophes rather than to the length of the string, which matters
/// for the case that reaches it in practice: JSON in a `data-*`
/// attribute value.
pub(crate) const fn xpath_literal(text: &str) -> Literal<'_> {
    Literal::affixed("", text, "")
}

/// An XPath literal, as [`xpath_literal`] and [`Literal::affixed`] build
/// it. Implements `Display` rather than being a `String` because every
/// literal is written straight into a larger expression.
#[derive(Clone, Copy)]
pub(crate) struct Literal<'a> {
    before: &'static str,
    text: &'a str,
    after: &'static str,
}

impl<'a> Literal<'a> {
    /// The literal for `before`, `text` and `after` concatenated, without
    /// building that string first. The affixes are the fixed padding some
    /// tests put around a value (`' '`, `'-'`) and must not contain a
    /// quote, so only `text` decides how the literal is quoted.
    pub(crate) const fn affixed(before: &'static str, text: &'a str, after: &'static str) -> Self {
        Literal {
            before,
            text,
            after,
        }
    }
}

impl fmt::Display for Literal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_assert!(
            !(self.before.contains(['\'', '"']) || self.after.contains(['\'', '"'])),
            "a literal's affixes never decide its quoting"
        );
        let Literal {
            before,
            text,
            after,
        } = *self;
        if !text.contains('\'') {
            return write!(f, "'{before}{text}{after}'");
        }
        if !text.contains('"') {
            return write!(f, "\"{before}{text}{after}\"");
        }
        // The concat() fallback splits the whole string, affixes
        // included; it is rare enough to build that string for.
        let whole;
        let mut rest = if before.is_empty() && after.is_empty() {
            text
        } else {
            whole = format!("{before}{text}{after}");
            &whole
        };
        f.write_str("concat(")?;
        let mut first = true;
        while !rest.is_empty() {
            // A run of apostrophes goes inside double quotes, and the
            // run up to the next apostrophe inside single ones.
            let (len, quote) = if rest.starts_with('\'') {
                (rest.len() - rest.trim_start_matches('\'').len(), '"')
            } else {
                (rest.find('\'').unwrap_or(rest.len()), '\'')
            };
            let (run, tail) = rest.split_at(len);
            if !first {
                f.write_str(",")?;
            }
            first = false;
            write!(f, "{quote}{run}{quote}")?;
            rest = tail;
        }
        f.write_str(")")
    }
}

/// One condition of a conjunction. `or_group` marks an expression with a
/// top-level `or`, which needs parentheses whenever it is joined to other
/// conditions with `and`.
#[derive(Clone, Debug)]
pub(crate) struct Condition {
    pub(crate) expr: String,
    pub(crate) or_group: bool,
}

impl Condition {
    /// A condition with no top-level `or`.
    pub(crate) fn plain(expr: impl Into<String>) -> Self {
        Condition {
            expr: expr.into(),
            or_group: false,
        }
    }

    /// A condition whose expression has a top-level `or`.
    pub(crate) fn or_group(expr: impl Into<String>) -> Self {
        Condition {
            expr: expr.into(),
            or_group: true,
        }
    }

    /// The never-matching condition.
    fn never() -> Self {
        Condition::plain("0")
    }

    fn is_never(&self) -> bool {
        self.expr == "0"
    }

    /// OR together a list of conditions, as the `:is()`/`:not()`/`of S`
    /// argument handling needs. The result is an or-group when anything
    /// was actually joined (or the single member already was one).
    ///
    /// An exactly repeated branch is kept once, the same rule
    /// `XPathExpr::write_condition` applies to a conjunction: `X or X`
    /// selects what `X` does, so `:is(a, a)` is `*[self::a]`. The
    /// or-group is decided by what is left after that, so a list that
    /// folds down to one branch is no longer parenthesized when it is
    /// conjoined.
    ///
    /// An empty list has no or-join, so the result is `None` rather than
    /// an empty expression: every caller already has to decide what an
    /// argument list that constrains nothing means (`:not()` of it is
    /// unmatchable, `:is()` of it is a no-op), and the `Option` is where
    /// that decision is made.
    pub(crate) fn join_or(mut conditions: Vec<Condition>) -> Option<Condition> {
        if conditions.len() == 1 {
            return conditions.pop();
        }
        let mut expr = String::new();
        let mut kept = 0usize;
        let mut first_or_group = false;
        for_each_distinct(
            &conditions,
            |c| c.expr.as_str(),
            |c| {
                if kept == 0 {
                    first_or_group = c.or_group;
                } else {
                    expr.push_str(" or ");
                }
                expr.push_str(&c.expr);
                kept += 1;
            },
        );
        (kept > 0).then_some(Condition {
            expr,
            or_group: kept > 1 || first_or_group,
        })
    }
}

/// The longest list [`for_each_distinct`] de-duplicates by pairwise
/// comparison. Past it the pairwise scan's quadratic cost outgrows a
/// hash set's fixed one; below it the hash set is the slower of the
/// two, and the lists nearly every selector produces are one or two
/// long.
const LINEAR_DISTINCT_MAX: usize = 16;

/// Call `f` on each condition whose `key` has not been seen earlier in
/// the list, in the order they were written.
///
/// This is the one super-linear step the translator would otherwise
/// have: a class list or `:is()` argument list of *n* conditions is *n*
/// same-length strings, so a pairwise scan compares ~n²/2 of them past a
/// shared prefix. Both strategies keep the first occurrence, so which
/// one runs never shows in the output. The pairwise scan compares each
/// condition against the list before it rather than against a list of
/// kept ones, so the common short list costs no allocation at all.
fn for_each_distinct<'a, K>(
    conditions: &'a [Condition],
    key: impl Fn(&'a Condition) -> K,
    mut f: impl FnMut(&'a Condition),
) where
    K: Eq + std::hash::Hash,
{
    if conditions.len() <= LINEAR_DISTINCT_MAX {
        for (i, condition) in conditions.iter().enumerate() {
            let k = key(condition);
            if !conditions[..i].iter().any(|earlier| key(earlier) == k) {
                f(condition);
            }
        }
    } else {
        let mut seen = std::collections::HashSet::with_capacity(conditions.len());
        for condition in conditions {
            if seen.insert(key(condition)) {
                f(condition);
            }
        }
    }
}

/// A partially built XPath expression: path, element, predicates, and
/// conditions.
#[derive(Clone, Debug)]
pub(crate) struct XPathExpr {
    pub(crate) path: String,
    /// The node test. Borrowed when it is a constant, which the
    /// universal `*` nearly always is.
    element: Cow<'static, str>,
    conditions: Vec<Condition>,
    /// Standalone predicates rendered each in its own bracket pair before
    /// the combined condition: `element[p1][p2][condition]`. Used where
    /// brackets must stay separate — e.g. the `+` combinator's `[1]`
    /// position test, which has to apply before any further filtering.
    predicates: Vec<Cow<'static, str>>,
    /// When an element name cannot be used as an XPath name test on its
    /// own — folded into a condition on `*`, or pinned by a condition
    /// alongside a `prefix:*` test — an equivalent node test for that
    /// name; `None` otherwise. Lets the of-type pseudo-classes
    /// distinguish such elements from the universal selector and count
    /// their siblings correctly.
    pub(crate) name_test: Option<String>,
    /// The local name a folded name test pins the subject to, set by
    /// whoever folds a name into a condition; see
    /// [`XPathExpr::local_name`], which reads the rest off the node test.
    pub(crate) folded_name: Option<String>,
}

impl XPathExpr {
    /// A new expression on `element`, which must be a usable XPath node
    /// test. The callers that fold a name into a condition instead set
    /// `folded_name` themselves.
    pub(crate) fn new(element: impl Into<Cow<'static, str>>) -> Self {
        XPathExpr {
            path: String::new(),
            element: element.into(),
            conditions: Vec::new(),
            predicates: Vec::new(),
            name_test: None,
            folded_name: None,
        }
    }

    /// The subject's local name, when the compound pins it to exactly
    /// one — whether by a plain node test (`input`, `h:input`) or by the
    /// condition a name needing quoting folds into. `None` for a
    /// wildcard subject (`*`, `ns|*`), which matches any local name.
    ///
    /// The HTML pseudo-class overrides identify elements by
    /// `local-name()`, so a pinned name decides every one of those tests
    /// at translation time and leaves only the arm that can match.
    ///
    /// Read off the node test rather than stored: a name moved into a
    /// `self::` test by [`XPathExpr::take_element_into_self_test`] is
    /// still the `name_test`, and a name test that is not a plain node
    /// test only ever comes with a `folded_name`. Only meaningful for a
    /// single compound; nothing asks once compounds are joined.
    pub(crate) fn local_name(&self) -> Option<&str> {
        if let Some(name) = &self.folded_name {
            return Some(name);
        }
        let node_test = self.name_test.as_deref().unwrap_or(&self.element);
        match node_test {
            "*" => None,
            _ if node_test.ends_with(":*") => None,
            _ => node_test.rsplit(':').next(),
        }
    }

    /// Render the whole expression — path, node test, predicates and
    /// the combined condition — onto the end of `out`. An empty `out`
    /// takes over the path's buffer rather than copying it.
    pub(crate) fn render_into(mut self, out: &mut String) {
        if out.is_empty() {
            *out = std::mem::take(&mut self.path);
        } else {
            out.push_str(&self.path);
        }
        self.render_tail(out);
    }

    /// Render everything the path is followed by — the node test, the
    /// standalone predicates, and the combined condition — onto `out`.
    fn render_tail(&self, out: &mut String) {
        out.push_str(&self.element);
        for predicate in &self.predicates {
            out.push('[');
            out.push_str(predicate);
            out.push(']');
        }
        if !self.conditions.is_empty() {
            out.push('[');
            self.write_condition(out, /* as_operand = */ false);
            out.push(']');
        }
    }

    /// Whether the conjunction contains a condition that cannot match,
    /// and so is that condition; see [`XPathExpr::write_condition`].
    pub(crate) fn is_never(&self) -> bool {
        self.conditions.iter().any(Condition::is_never)
    }

    /// Whether any condition has been added.
    pub(crate) fn has_condition(&self) -> bool {
        !self.conditions.is_empty()
    }

    /// Write the conjunction of every added condition onto `out`: one
    /// passes through untouched (brackets and `not(...)` need no
    /// parentheses around a lone or-group), several join with `and`,
    /// parenthesizing the or-groups among them. With `as_operand`, the
    /// conjunction is about to be conjoined with something else, so a
    /// lone or-group is parenthesized too.
    ///
    /// Returns whether what was written is an or-group — a lone one,
    /// left unparenthesized — or `None` if there were no conditions and
    /// nothing was written.
    ///
    /// Two simplifications of the conjunction happen here, so that a
    /// reader of the output does not have to reason about a boolean to
    /// see what a compound does. Both are local to one conjunction, and
    /// neither can change which nodes it selects:
    ///
    /// - a condition that is literally `0` (what a pseudo-class that
    ///   cannot match statically emits) absorbs the rest, so
    ///   `a:hover[x]` is `a[0]` rather than `a[0 and @x]`;
    /// - an exactly repeated condition — same expression, same
    ///   or-group-ness — is kept once, so `a[href]:any-link` is
    ///   `a[@href]` rather than `a[@href and @href]`.
    ///
    /// The standalone predicates are deliberately untouched: they are
    /// separate brackets because their position matters (the `+`
    /// combinator's `[1]`), so a `0` here says nothing about them.
    ///
    /// The `0` absorption has a counterpart one level up, in
    /// `Translator::argument_condition`: the conjunction there is
    /// assembled from a chain of compounds rather than held in one
    /// `conditions`, so that chain applies the same rule for itself.
    pub(crate) fn write_condition(&self, out: &mut String, as_operand: bool) -> Option<bool> {
        if self.conditions.is_empty() {
            return None;
        }
        // Nothing conjoined with a never-matching condition can bring it
        // back, so the whole conjunction is that condition.
        if self.is_never() {
            out.push('0');
            return Some(false);
        }
        let start = out.len();
        let mut kept = 0usize;
        let mut last_or_group = false;
        for_each_distinct(
            &self.conditions,
            |c| (c.expr.as_str(), c.or_group),
            |c| {
                if kept > 0 {
                    out.push_str(" and ");
                }
                // Every or-group is parenthesized, since whether it
                // turns out to be alone is only known at the end; a lone
                // one has its parentheses taken back off below.
                if c.or_group {
                    out.push('(');
                    out.push_str(&c.expr);
                    out.push(')');
                } else {
                    out.push_str(&c.expr);
                }
                last_or_group = c.or_group;
                kept += 1;
            },
        );
        if kept == 1 && last_or_group && !as_operand {
            // The lone or-group needs no parentheses after all.
            out.remove(start);
            out.pop();
            return Some(true);
        }
        Some(false)
    }

    /// The conjunction of every added condition as one [`Condition`]
    /// (see [`XPathExpr::write_condition`]), or `None` if there is none.
    pub(crate) fn into_condition(mut self) -> Option<Condition> {
        if self.is_never() {
            return Some(Condition::never());
        }
        if self.conditions.len() == 1 {
            return self.conditions.pop();
        }
        let mut expr = String::new();
        let or_group = self.write_condition(&mut expr, false)?;
        Some(Condition { expr, or_group })
    }

    pub(crate) fn add_predicate(&mut self, predicate: impl Into<Cow<'static, str>>) {
        self.predicates.push(predicate.into());
    }

    /// Add one condition to the conjunction. The expression must not
    /// contain a top-level `or` — those go through `add_or_condition` so
    /// rendering knows to parenthesize them.
    pub(crate) fn add_condition(&mut self, condition: impl Into<String>) {
        self.push_condition(Condition::plain(condition));
    }

    /// Add a condition whose expression contains a top-level `or`.
    pub(crate) fn add_or_condition(&mut self, condition: impl Into<String>) {
        self.push_condition(Condition::or_group(condition));
    }

    pub(crate) fn push_condition(&mut self, condition: Condition) {
        self.conditions.push(condition);
    }

    /// Move the element name out of the node test and into a `self::`
    /// condition, leaving the node test `*`. Used where a compound has to
    /// become a predicate on a candidate element (a functional
    /// pseudo-class argument) or where a position predicate must count
    /// every sibling (`+`). `self::e` tests exactly what the name tested
    /// as a node test, so a bare name still matches only the null
    /// namespace and a prefixed one still resolves through the caller's
    /// namespace map.
    pub(crate) fn take_element_into_self_test(&mut self) {
        if self.element == "*" {
            return;
        }
        let element = std::mem::replace(&mut self.element, Cow::Borrowed("*"));
        self.add_condition(format!("self::{element}"));
        // The name was a usable node test, so it stays the of-type
        // nodetest — unless one was already pinned alongside it, as for a
        // prefixed wildcard carrying a local-name() test.
        self.name_test.get_or_insert_with(|| element.into_owned());
    }

    /// The node test selecting siblings of the same type, for the of-type
    /// pseudo-classes. `None` when the subject is a wildcard, prefixed
    /// or not, and so has no single type.
    pub(crate) fn same_type_nodetest(&self) -> Option<String> {
        match &self.name_test {
            // A name test is set whenever the element alone is not the
            // whole story: either it was folded into a condition on `*`,
            // or it is a prefixed wildcard pinned by a local-name() test.
            Some(name_test) => Some(name_test.clone()),
            // A wildcard subject has no single type to count siblings
            // by: `ns|*` matches every name in that namespace, so
            // counting `ns|*` siblings would be "position among elements
            // in the namespace", not among elements of the same type.
            None if self.element != "*" && !self.element.ends_with(":*") => {
                Some(self.element.clone().into_owned())
            }
            None => None,
        }
    }

    /// Append `combiner` and `other` to this expression, taking over
    /// `other`'s node test, predicates and conditions.
    pub(crate) fn join(&mut self, combiner: &str, other: XPathExpr) {
        // Grow the accumulated path in place rather than re-rendering it:
        // rendering the whole expression per combinator would copy the
        // path again for each one, so an n-compound chain would cost
        // O(n^2) bytes.
        let mut path = std::mem::take(&mut self.path);
        self.render_tail(&mut path);
        path.push_str(combiner);
        // A compound's own path is always empty; only `join` and
        // `selector_to_xpath`'s head ever set one, and neither result is
        // passed here as `other`.
        path.push_str(&other.path);
        *self = XPathExpr { path, ..other };
    }

    /// Replace the node test with `*`, returning what it was.
    pub(crate) fn take_element(&mut self) -> Cow<'static, str> {
        std::mem::replace(&mut self.element, Cow::Borrowed("*"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl XPathExpr {
        fn render(&self) -> String {
            let mut out = String::new();
            self.clone().render_into(&mut out);
            out
        }
    }

    #[test]
    fn safe_names() {
        assert!(is_safe_name("div"));
        assert!(is_safe_name("_x"));
        assert!(is_safe_name("a-b.c_1"));
        assert!(!is_safe_name("1a"));
        assert!(!is_safe_name("di[v"));
        assert!(!is_safe_name("di\u{a0}v"));
        assert!(!is_safe_name(""));
    }

    fn xpath_literal_string(text: &str) -> String {
        xpath_literal(text).to_string()
    }

    #[test]
    fn literals() {
        assert_eq!(xpath_literal_string("foo"), "'foo'");
        assert_eq!(xpath_literal_string("f'oo"), "\"f'oo\"");
        // both quote kinds: maximal runs, not one character per argument
        assert_eq!(xpath_literal_string("f'o\"o"), "concat('f',\"'\",'o\"o')");
        assert_eq!(
            xpath_literal_string("it's \"q\""),
            "concat('it',\"'\",'s \"q\"')"
        );
        // a leading and a doubled apostrophe
        assert_eq!(xpath_literal_string("''a\"b"), "concat(\"''\",'a\"b')");
        // affixes are quoted with the text, and join its first and last
        // runs in the concat() fallback
        assert_eq!(Literal::affixed(" ", "a", " ").to_string(), "' a '");
        assert_eq!(Literal::affixed("", "a'b", "-").to_string(), "\"a'b-\"");
        assert_eq!(
            Literal::affixed(" ", "'a\"", " ").to_string(),
            "concat(' ',\"'\",'a\" ')"
        );
    }

    #[test]
    fn condition_parens() {
        let mut xp = XPathExpr::new("e");
        xp.add_condition("@foo = 'bar'");
        assert_eq!(xp.render(), "e[@foo = 'bar']");
        xp.add_condition("@baz");
        assert_eq!(xp.render(), "e[@foo = 'bar' and @baz]");

        // a lone or-group needs no parentheses inside the brackets, a
        // conjoined one does
        let mut xp = XPathExpr::new("e");
        xp.add_or_condition("@a or @b");
        assert_eq!(xp.render(), "e[@a or @b]");
        xp.add_condition("@c");
        assert_eq!(xp.render(), "e[(@a or @b) and @c]");
    }

    #[test]
    fn never_matching_condition_absorbs_the_conjunction() {
        let mut xp = XPathExpr::new("a");
        xp.add_condition("@x");
        xp.add_condition("0");
        xp.add_or_condition("@a or @b");
        assert_eq!(xp.render(), "a[0]");

        // the standalone predicates keep their own brackets: `0` says
        // nothing about a position test that applies before it
        let mut xp = XPathExpr::new("*");
        xp.add_predicate("1");
        xp.add_condition("0");
        assert_eq!(xp.render(), "*[1][0]");
    }

    #[test]
    fn duplicate_conditions_are_kept_once() {
        let mut xp = XPathExpr::new("a");
        xp.add_condition("@href");
        xp.add_condition("@href");
        assert_eq!(xp.render(), "a[@href]");

        xp.add_condition("@x");
        xp.add_condition("@href");
        assert_eq!(xp.render(), "a[@href and @x]");

        // same expression, different or-group-ness: not a duplicate,
        // since only one of the two is parenthesized
        let mut xp = XPathExpr::new("a");
        xp.add_or_condition("@a or @b");
        xp.add_or_condition("@a or @b");
        xp.add_condition("@c");
        assert_eq!(xp.render(), "a[(@a or @b) and @c]");
    }

    #[test]
    fn predicates_render_separately_before_condition() {
        let mut xp = XPathExpr::new("*");
        xp.add_predicate("1");
        xp.add_predicate("self::f");
        assert_eq!(xp.render(), "*[1][self::f]");
        xp.add_condition("@bar");
        assert_eq!(xp.render(), "*[1][self::f][@bar]");

        // join bakes the left side's predicates into the path and takes
        // over the right side's.
        let other = XPathExpr::new("g");
        xp.join("/following-sibling::", other);
        assert_eq!(xp.render(), "*[1][self::f][@bar]/following-sibling::g");
        xp.add_predicate("1");
        assert_eq!(xp.render(), "*[1][self::f][@bar]/following-sibling::g[1]");
    }
}
