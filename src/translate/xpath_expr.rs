//! The `XPathExpr` builder and string helpers.
//!
//! Conditions are stored unparenthesized and parenthesized only at render
//! time, and only where XPath precedence requires it: an expression with a
//! top-level `or` (a `Condition` with `or_group` set) is wrapped when it
//! is conjoined with other conditions, since `and` binds tighter than
//! `or`. The exact output (like `e[@foo = 'bar']`) and the `*/`-collapse
//! guard in `join` are load-bearing for the crate's output contract and
//! are pinned by tests.

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

/// Quote a string as an XPath literal.
///
/// Note: each character is quoted individually in the `concat(...)`
/// branch — a quirk preserved because the exact output is pinned by tests.
pub fn xpath_literal(literal: &str) -> String {
    if !literal.contains('\'') {
        format!("'{literal}'")
    } else if !literal.contains('"') {
        format!("\"{literal}\"")
    } else {
        let parts: Vec<String> = literal
            .chars()
            .map(|c| {
                if c == '\'' {
                    format!("\"{c}\"")
                } else {
                    format!("'{c}'")
                }
            })
            .collect();
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
    /// When an element name cannot be used as an XPath name test (and so
    /// `element` has been folded into a condition on `*`), an equivalent
    /// node test for that name; `None` otherwise. Lets the of-type
    /// pseudo-classes distinguish such elements from the universal
    /// selector and count their siblings correctly.
    pub name_test: Option<String>,
}

impl XPathExpr {
    pub fn new(element: &str) -> Self {
        XPathExpr {
            path: String::new(),
            element: element.to_owned(),
            conditions: Vec::new(),
            predicates: Vec::new(),
            name_test: None,
        }
    }

    pub fn str(&self) -> String {
        let mut p = format!("{}{}", self.path, self.element);
        for predicate in &self.predicates {
            p.push('[');
            p.push_str(predicate);
            p.push(']');
        }
        if let Some(condition) = self.condition() {
            p.push('[');
            p.push_str(&condition.expr);
            p.push(']');
        }
        p
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

    pub fn add_name_test(&mut self) {
        if self.element == "*" {
            return;
        }
        let cond = format!("name() = {}", xpath_literal(&self.element));
        self.name_test = Some(format!("*[{cond}]"));
        self.add_condition(&cond);
        self.element = "*".to_owned();
    }

    /// The node test selecting siblings of the same type, for the of-type
    /// pseudo-classes. `None` when the element is a genuine universal.
    pub fn same_type_nodetest(&self) -> Option<String> {
        if self.element != "*" {
            Some(self.element.clone())
        } else {
            self.name_test.clone()
        }
    }

    /// Append `combiner` and `other` to this expression, collapsing a
    /// leading `*/` in `other`'s path.
    pub fn join(&mut self, combiner: &str, other: &XPathExpr) {
        let mut p = format!("{}{}", self.str(), combiner);
        if other.path != "*/" {
            p.push_str(&other.path);
        }
        self.path = p;
        self.element = other.element.clone();
        self.conditions = other.conditions.clone();
        self.predicates = other.predicates.clone();
        self.name_test = other.name_test.clone();
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
        assert_eq!(xpath_literal("f'o\"o"), "concat('f',\"'\",'o','\"','o')");
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
