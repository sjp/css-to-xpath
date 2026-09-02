//! `:scope`, and how the caller's path prefix is applied to each
//! branch of a selector group.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

/// :scope (Selectors Level 4) anchors the expression at the node the
/// XPath is evaluated from: the leftmost compound moves onto the
/// self:: axis and the prefix is not applied.
#[test]
fn scope_pseudo() {
    let mut t = Cases::new(Mode::Generic);
    t.check(":scope", "self::*");
    t.check(":ScoPE", "self::*");
    t.check(":scope > a", "self::*/a");
    t.check(":scope a", "self::*//a");
    t.check(":scope + a", "self::*/following-sibling::*[1][self::a]");
    t.check(":scope ~ a", "self::*/following-sibling::a");
    // Other simple selectors in the :scope compound constrain the
    // context node itself.
    t.check("div:scope", "self::div");
    t.check("svg|g:scope", "self::svg:g");
    t.check(
        ":scope.foo > a",
        "self::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')]/a",
    );
    t.check(
        ":scope:first-child",
        "self::*[count(preceding-sibling::*) = 0]",
    );
    // The prefix is replaced by the self:: anchor, per selector group.
    assert_eq!(
        t.css_to_xpath(":scope > a", "descendant-or-self::")
            .unwrap(),
        "self::*/a"
    );
    assert_eq!(
        t.css_to_xpath("a, :scope > b", "descendant-or-self::")
            .unwrap(),
        "descendant-or-self::a | self::*/b"
    );
}

/// The prefix opens each branch of a selector group, and what it opens
/// is the branch's first *step* — a leftmost compound whose pseudo-class
/// becomes a predicate still takes the prefix on its node test, and a
/// compound that is a bare `*` node test with predicates is no
/// exception.
#[test]
fn prefix_applied_per_branch() {
    let mut t = Cases::with_prefix(Mode::Generic, "descendant-or-self::");
    t.check("a, b", "descendant-or-self::a | descendant-or-self::b");
    t.check("a:has(> b)", "descendant-or-self::a[child::b]");
    t.check(
        ":is(a, b) > c",
        "descendant-or-self::*[self::a or self::b]/c",
    );

    let mut root = Cases::with_prefix(Mode::Generic, "//");
    root.check(":is(a, b) > c", "//*[self::a or self::b]/c");
    root.check("a:has(> b)", "//a[child::b]");
}
