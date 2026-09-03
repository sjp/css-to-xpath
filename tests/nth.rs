//! The `:nth-*` family: An+B arithmetic, the of-type variants, and
//! `:nth-child(An+B of S)`.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

/// The nth-* family and its an+b arithmetic.
#[test]
fn nth_family() {
    let mut t = Cases::new(Mode::Generic);
    t.check("e:nth-child(1)", "e[count(preceding-sibling::*) = 0]");
    t.check(
        "e:nth-child(3n+2)",
        "e[count(preceding-sibling::*) >= 1 and (count(preceding-sibling::*) +2) mod 3 = 0]",
    );
    t.check(
        "e:nth-child(3n-2)",
        "e[count(preceding-sibling::*) mod 3 = 0]",
    );
    t.check("e:nth-child(-n+6)", "e[count(preceding-sibling::*) <= 5]");
    t.check("e:nth-child(n)", "e");
    t.check("e:nth-child(odd)", t.xpath("e:nth-child(2n+1)"));
    t.check("e:nth-child(even)", t.xpath("e:nth-child(2n)"));
    // An+B is ASCII case-insensitive per css-syntax; Servo handles it
    // natively.
    t.check("e:nth-child(2N)", t.xpath("e:nth-child(2n)"));
    t.check("e:nth-child(ODD)", t.xpath("e:nth-child(odd)"));
    t.check("e:nth-child(EVEN)", t.xpath("e:nth-child(even)"));
    t.check("e:nth-child(-N+3)", t.xpath("e:nth-child(-n+3)"));
    t.check("e:nth-last-child(1)", "e[count(following-sibling::*) = 0]");
    t.check(
        "e:nth-last-child(2n)",
        "e[(count(following-sibling::*) +1) mod 2 = 0]",
    );
    t.check(
        "e:nth-last-child(2n+1)",
        "e[count(following-sibling::*) mod 2 = 0]",
    );
    t.check(
        "e:nth-last-child(2n+2)",
        "e[count(following-sibling::*) >= 1 and (count(following-sibling::*) +1) mod 2 = 0]",
    );
    t.check(
        "e:nth-last-child(3n+1)",
        "e[count(following-sibling::*) mod 3 = 0]",
    );
    t.check(
        "e:nth-last-child(-n+2)",
        "e[count(following-sibling::*) <= 1]",
    );
    t.check("e:nth-of-type(1)", "e[count(preceding-sibling::e) = 0]");
    t.check(
        "e:nth-last-of-type(1)",
        "e[count(following-sibling::e) = 0]",
    );
    t.check("div e:nth-last-of-type(1) .aclass", "div//e[count(following-sibling::e) = 0]//*[contains(concat(' ', normalize-space(@class), ' '), ' aclass ')]");
    // Servo collapses :first-child & co. into nth data; the general
    // an+b form covers them (see translate::nth).
    t.check("e:first-child", "e[count(preceding-sibling::*) = 0]");
    t.check("e:last-child", "e[count(following-sibling::*) = 0]");
    t.check("e:first-of-type", "e[count(preceding-sibling::e) = 0]");
    t.check("e:last-of-type", "e[count(following-sibling::e) = 0]");
    t.check(
        "e:only-child",
        "e[count(preceding-sibling::*) = 0 and count(following-sibling::*) = 0]",
    );
    t.check(
        "e:only-of-type",
        "e[count(preceding-sibling::e) = 0 and count(following-sibling::e) = 0]",
    );
    // Element names needing quoting fold into a namespace-pinned
    // name() condition; the of-type pseudos count same-type siblings
    // through the same test.
    t.check("é:first-of-type", "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]");
    t.check("é:nth-of-type(2)", "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 1]");
    t.check("é:nth-last-of-type(1)", "*[name() = 'é' and namespace-uri() = '' and count(following-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]");
    t.check("é:only-of-type", "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 0 and count(following-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]");
    // Explicit-namespace and no-namespace elements go through
    // local-name()/name()-plus-namespace-uri() the same way.
    t.check(
        "*|e:first-of-type",
        "*[local-name() = 'e' and count(preceding-sibling::*[local-name() = 'e']) = 0]",
    );
    t.check("|é:first-of-type", "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]");
    t.check(
        "e ~ f:nth-child(3)",
        "e/following-sibling::f[count(preceding-sibling::*) = 2]",
    );
    // Early exits: a=1 with b<=1 matches everything; a<0 with b<1 is
    // impossible.
    t.check("e:nth-child(n+1)", "e");
    t.check("e:nth-child(n-5)", "e");
    t.check("e:nth-child(-n)", "e[0]");
    // a == 0 with b <= 0 is impossible too, and says so rather than
    // asking for a negative sibling count.
    t.check("e:nth-child(0)", "e[0]");
    t.check("e:nth-child(0n+0)", "e[0]");
    t.check("e:nth-child(-3)", "e[0]");
    t.check("e:nth-last-child(0)", "e[0]");
    t.check("e:nth-of-type(0)", "e[0]");
    t.check("e:nth-child(-2n-1)", "e[0]");
    t.check("e:nth-child(-n+0)", "e[0]");
    t.check("e:nth-child(-n+1)", "e[count(preceding-sibling::*) <= 0]");
    t.check(
        "e:nth-child(-2n+2)",
        "e[count(preceding-sibling::*) <= 1 and (count(preceding-sibling::*) +1) mod -2 = 0]",
    );
}

/// `of S` selector lists (CSS Level 4), nth-child only.
#[test]
fn nth_child_of() {
    let mut t = Cases::new(Mode::Generic);
    t.check("div:nth-child(2 of .foo)", "div[count(preceding-sibling::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')]) = 1 and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]");
    // a=1, b<=1: only the current-element check remains.
    t.check(
        "li:nth-child(n of .item)",
        "li[contains(concat(' ', normalize-space(@class), ' '), ' item ')]",
    );
    // An impossible series never matches, whatever the `of` list says,
    // so the 0 absorbs the current-element check.
    t.check("li:nth-child(-n of .item)", "li[0]");
    // An element argument folds into a self:: test.
    t.check("div:nth-child(2 of div.foo)", "div[count(preceding-sibling::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::div]) = 1 and contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::div]");
    // A universal argument makes the list match everything, like a
    // plain :nth-child.
    t.check(
        "li:nth-child(2 of .foo, *)",
        "li[count(preceding-sibling::*) = 1]",
    );
}
