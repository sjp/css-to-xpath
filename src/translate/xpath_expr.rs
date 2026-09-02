//! The `XPathExpr` builder and string helpers.
//!
//! Conditions are stored unparenthesized and parenthesized only at render
//! time, and only where XPath precedence requires it: an expression with a
//! top-level `or` (a `Condition` with `or_group` set) is wrapped when it
//! is conjoined with other conditions, since `and` binds tighter than
//! `or`. The exact output (like `e[@foo = 'bar']`) is load-bearing for
//! the crate's output contract and is pinned by tests.

/// Whether a name can be used directly in an XPath name test (no quoting
/// needed).
pub fn is_safe_name(name: &str) -> bool {
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
pub fn ascii_lower(subject: &str) -> String {
    format!(
        "translate({subject}, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz')"
    )
}

/// Quote a string as an XPath literal.
///
/// XPath 1.0 literals have no escape syntax, so a string containing both
/// quote kinds cannot be written as one literal and has to be
/// `concat()`ed from several. Splitting it into *maximal* runs — each
/// run of apostrophes quoted with `"`, everything between them quoted
/// with `'` — keeps that fallback proportional to the number of
/// apostrophes rather than to the length of the string, which matters
/// for the case that reaches it in practice: JSON in a `data-*`
/// attribute value.
pub fn xpath_literal(literal: &str) -> String {
    if !literal.contains('\'') {
        format!("'{literal}'")
    } else if !literal.contains('"') {
        format!("\"{literal}\"")
    } else {
        let mut parts: Vec<String> = Vec::new();
        let mut rest = literal;
        while !rest.is_empty() {
            // A run of apostrophes goes inside double quotes, and the
            // run up to the next apostrophe inside single ones.
            let (len, quote) = if rest.starts_with('\'') {
                (rest.len() - rest.trim_start_matches('\'').len(), '"')
            } else {
                (rest.find('\'').unwrap_or(rest.len()), '\'')
            };
            let (run, tail) = rest.split_at(len);
            parts.push(format!("{quote}{run}{quote}"));
            rest = tail;
        }
        format!("concat({})", parts.join(","))
    }
}

/// One condition of a conjunction. `or_group` marks an expression with a
/// top-level `or`, which needs parentheses whenever it is joined to other
/// conditions with `and`.
#[derive(Clone, Debug)]
pub struct Condition {
    pub expr: String,
    pub or_group: bool,
}

impl Condition {
    /// OR together a list of conditions, as the `:is()`/`:not()`/`of S`
    /// argument handling needs. The result is an or-group when anything
    /// was actually joined (or the single member already was one).
    pub fn join_or(conditions: &[Condition]) -> Condition {
        let exprs: Vec<&str> = conditions.iter().map(|c| c.expr.as_str()).collect();
        Condition {
            expr: exprs.join(" or "),
            or_group: conditions.len() > 1 || conditions[0].or_group,
        }
    }
}

/// A partially built XPath expression: path, element, predicates, and
/// conditions.
#[derive(Clone, Debug)]
pub struct XPathExpr {
    pub path: String,
    pub element: String,
    conditions: Vec<Condition>,
    /// Standalone predicates rendered each in its own bracket pair before
    /// the combined condition: `element[p1][p2][condition]`. Used where
    /// brackets must stay separate — e.g. the `+` combinator's `[1]`
    /// position test, which has to apply before any further filtering.
    predicates: Vec<String>,
    /// When an element name cannot be used as an XPath name test on its
    /// own — folded into a condition on `*`, or pinned by a condition
    /// alongside a `prefix:*` test — an equivalent node test for that
    /// name; `None` otherwise. Lets the of-type pseudo-classes
    /// distinguish such elements from the universal selector and count
    /// their siblings correctly.
    pub name_test: Option<String>,
    /// The subject's local name, when the compound pins it to exactly
    /// one — whether by a plain node test (`input`, `h:input`) or by the
    /// condition a name needing quoting folds into. `None` for a
    /// wildcard subject (`*`, `ns|*`), which matches any local name.
    ///
    /// The HTML pseudo-class overrides identify elements by
    /// `local-name()`, so a pinned name decides every one of those tests
    /// at translation time and leaves only the arm that can match.
    pub local_name: Option<String>,
}

impl XPathExpr {
    /// A new expression on `element`, which must be a usable XPath node
    /// test. The local name is read straight off it; the callers that
    /// fold a name into a condition instead set `local_name` themselves.
    pub fn new(element: &str) -> Self {
        let local_name = match element {
            "*" => None,
            _ if element.ends_with(":*") => None,
            _ => Some(element.rsplit(':').next().unwrap_or(element).to_owned()),
        };
        XPathExpr {
            path: String::new(),
            element: element.to_owned(),
            conditions: Vec::new(),
            predicates: Vec::new(),
            name_test: None,
            local_name,
        }
    }

    pub fn str(&self) -> String {
        let mut p = self.path.clone();
        self.render_tail(&mut p);
        p
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
        if let Some(condition) = self.condition() {
            out.push('[');
            out.push_str(&condition.expr);
            out.push(']');
        }
    }

    /// The conjunction of every added condition: one passes through
    /// untouched (brackets and `not(...)` need no parentheses around a
    /// lone or-group), several join with `and`, parenthesizing the
    /// or-groups among them.
    pub fn condition(&self) -> Option<Condition> {
        match self.conditions.len() {
            0 => None,
            1 => Some(self.conditions[0].clone()),
            _ => {
                let parts: Vec<String> = self
                    .conditions
                    .iter()
                    .map(|c| {
                        if c.or_group {
                            format!("({})", c.expr)
                        } else {
                            c.expr.clone()
                        }
                    })
                    .collect();
                Some(Condition {
                    expr: parts.join(" and "),
                    or_group: false,
                })
            }
        }
    }

    pub fn add_predicate(&mut self, predicate: &str) {
        self.predicates.push(predicate.to_owned());
    }

    /// Add one condition to the conjunction. The expression must not
    /// contain a top-level `or` — those go through `add_or_condition` so
    /// rendering knows to parenthesize them.
    pub fn add_condition(&mut self, condition: &str) {
        self.push_condition(Condition {
            expr: condition.to_owned(),
            or_group: false,
        });
    }

    /// Add a condition whose expression contains a top-level `or`.
    pub fn add_or_condition(&mut self, condition: &str) {
        self.push_condition(Condition {
            expr: condition.to_owned(),
            or_group: true,
        });
    }

    pub fn push_condition(&mut self, condition: Condition) {
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
    pub fn take_element_into_self_test(&mut self) {
        if self.element == "*" {
            return;
        }
        let element = std::mem::replace(&mut self.element, "*".to_owned());
        self.add_condition(&format!("self::{element}"));
        // The name was a usable node test, so it stays the of-type
        // nodetest — unless one was already pinned alongside it, as for a
        // prefixed wildcard carrying a local-name() test.
        self.name_test.get_or_insert(element);
    }

    /// The node test selecting siblings of the same type, for the of-type
    /// pseudo-classes. `None` when the subject is a wildcard, prefixed
    /// or not, and so has no single type.
    pub fn same_type_nodetest(&self) -> Option<String> {
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
                Some(self.element.clone())
            }
            None => None,
        }
    }

    /// Append `combiner` and `other` to this expression, taking over
    /// `other`'s node test, predicates and conditions.
    pub fn join(&mut self, combiner: &str, other: &XPathExpr) {
        // Grow the accumulated path in place rather than re-rendering it:
        // rendering the whole expression per combinator would copy the
        // path again for each one, so an n-compound chain would cost
        // O(n^2) bytes.
        let mut path = std::mem::take(&mut self.path);
        self.render_tail(&mut path);
        path.push_str(combiner);
        // A compound's own path is always empty; only `join` and the
        // `:scope` anchor ever set one, and neither result is passed here
        // as `other`.
        path.push_str(&other.path);
        self.path = path;
        self.element = other.element.clone();
        self.conditions = other.conditions.clone();
        self.predicates = other.predicates.clone();
        self.name_test = other.name_test.clone();
        self.local_name = other.local_name.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn literals() {
        assert_eq!(xpath_literal("foo"), "'foo'");
        assert_eq!(xpath_literal("f'oo"), "\"f'oo\"");
        // both quote kinds: maximal runs, not one character per argument
        assert_eq!(xpath_literal("f'o\"o"), "concat('f',\"'\",'o\"o')");
        assert_eq!(xpath_literal("it's \"q\""), "concat('it',\"'\",'s \"q\"')");
        // a leading and a doubled apostrophe
        assert_eq!(xpath_literal("''a\"b"), "concat(\"''\",'a\"b')");
    }

    #[test]
    fn condition_parens() {
        let mut xp = XPathExpr::new("e");
        xp.add_condition("@foo = 'bar'");
        assert_eq!(xp.str(), "e[@foo = 'bar']");
        xp.add_condition("@baz");
        assert_eq!(xp.str(), "e[@foo = 'bar' and @baz]");

        // a lone or-group needs no parentheses inside the brackets, a
        // conjoined one does
        let mut xp = XPathExpr::new("e");
        xp.add_or_condition("@a or @b");
        assert_eq!(xp.str(), "e[@a or @b]");
        xp.add_condition("@c");
        assert_eq!(xp.str(), "e[(@a or @b) and @c]");
    }

    #[test]
    fn predicates_render_separately_before_condition() {
        let mut xp = XPathExpr::new("*");
        xp.add_predicate("1");
        xp.add_predicate("self::f");
        assert_eq!(xp.str(), "*[1][self::f]");
        xp.add_condition("@bar");
        assert_eq!(xp.str(), "*[1][self::f][@bar]");

        // join bakes the left side's predicates into the path and takes
        // over the right side's.
        let other = XPathExpr::new("g");
        xp.join("/following-sibling::", &other);
        assert_eq!(xp.str(), "*[1][self::f][@bar]/following-sibling::g");
        xp.add_predicate("1");
        assert_eq!(xp.str(), "*[1][self::f][@bar]/following-sibling::g[1]");
    }
}
