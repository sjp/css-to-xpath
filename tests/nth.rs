//! The `:nth-*` family: An+B arithmetic, the of-type variants, and
//! `:nth-child(An+B of S)`.

mod cases;
use cases::Cases;
use css_to_xpath::{Error, Mode};

/// The nth-* family and its an+b arithmetic.
#[test]
fn nth_family() {
    let mut t = Cases::new(Mode::Generic);
    t.check("e:nth-child(1)", "e[not(preceding-sibling::*[1])]");
    t.check(
        "e:nth-child(3n+2)",
        "e[(count(preceding-sibling::*) + 2) mod 3 = 0]",
    );
    t.check(
        "e:nth-child(3n-2)",
        "e[count(preceding-sibling::*) mod 3 = 0]",
    );
    t.check("e:nth-child(-n+6)", "e[not(preceding-sibling::*[6])]");
    t.check("e:nth-child(n)", "e");
    t.check("e:nth-child(odd)", t.xpath("e:nth-child(2n+1)"));
    t.check("e:nth-child(even)", t.xpath("e:nth-child(2n)"));
    // An+B is ASCII case-insensitive per css-syntax; Servo handles it
    // natively.
    t.check("e:nth-child(2N)", t.xpath("e:nth-child(2n)"));
    t.check("e:nth-child(ODD)", t.xpath("e:nth-child(odd)"));
    t.check("e:nth-child(EVEN)", t.xpath("e:nth-child(even)"));
    t.check("e:nth-child(-N+3)", t.xpath("e:nth-child(-n+3)"));
    t.check("e:nth-last-child(1)", "e[not(following-sibling::*[1])]");
    t.check(
        "e:nth-last-child(2n)",
        "e[(count(following-sibling::*) + 1) mod 2 = 0]",
    );
    t.check(
        "e:nth-last-child(2n+1)",
        "e[count(following-sibling::*) mod 2 = 0]",
    );
    t.check(
        "e:nth-last-child(2n+2)",
        "e[(count(following-sibling::*) + 1) mod 2 = 0]",
    );
    t.check(
        "e:nth-last-child(3n+1)",
        "e[count(following-sibling::*) mod 3 = 0]",
    );
    t.check("e:nth-last-child(-n+2)", "e[not(following-sibling::*[2])]");
    t.check("e:nth-of-type(1)", "e[not(preceding-sibling::e[1])]");
    t.check("e:nth-last-of-type(1)", "e[not(following-sibling::e[1])]");
    t.check("div e:nth-last-of-type(1) .aclass", "div//e[not(following-sibling::e[1])]//*[contains(concat(' ', normalize-space(@class), ' '), ' aclass ')]");
    // Servo collapses :first-child & co. into nth data; the general
    // an+b form covers them (see translate::nth).
    t.check("e:first-child", "e[not(preceding-sibling::*[1])]");
    t.check("e:last-child", "e[not(following-sibling::*[1])]");
    t.check("e:first-of-type", "e[not(preceding-sibling::e[1])]");
    t.check("e:last-of-type", "e[not(following-sibling::e[1])]");
    t.check(
        "e:only-child",
        "e[not(preceding-sibling::*[1]) and not(following-sibling::*[1])]",
    );
    t.check(
        "e:only-of-type",
        "e[not(preceding-sibling::e[1]) and not(following-sibling::e[1])]",
    );
    // Element names needing quoting fold into a namespace-pinned
    // name() condition; the of-type pseudos count same-type siblings
    // through the same test.
    t.check("é:first-of-type", "*[name() = 'é' and namespace-uri() = '' and not(preceding-sibling::*[name() = 'é' and namespace-uri() = ''][1])]");
    t.check("é:nth-of-type(2)", "*[name() = 'é' and namespace-uri() = '' and preceding-sibling::*[name() = 'é' and namespace-uri() = ''][1] and not(preceding-sibling::*[name() = 'é' and namespace-uri() = ''][2])]");
    t.check("é:nth-last-of-type(1)", "*[name() = 'é' and namespace-uri() = '' and not(following-sibling::*[name() = 'é' and namespace-uri() = ''][1])]");
    t.check("é:only-of-type", "*[name() = 'é' and namespace-uri() = '' and not(preceding-sibling::*[name() = 'é' and namespace-uri() = ''][1]) and not(following-sibling::*[name() = 'é' and namespace-uri() = ''][1])]");
    // A namespaced or no-namespace element is counted by the node test
    // it translates to.
    t.check(
        "ns|e:first-of-type",
        "ns:e[not(preceding-sibling::ns:e[1])]",
    );
    t.check("|e:first-of-type", "e[not(preceding-sibling::e[1])]");
    t.check(
        "ns|é:first-of-type",
        "ns:*[local-name() = 'é' and not(preceding-sibling::ns:*[local-name() = 'é'][1])]",
    );
    t.check("|é:first-of-type", "*[name() = 'é' and namespace-uri() = '' and not(preceding-sibling::*[name() = 'é' and namespace-uri() = ''][1])]");
    t.check(
        "e ~ f:nth-child(3)",
        "e/following-sibling::f[preceding-sibling::*[2] and not(preceding-sibling::*[3])]",
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
    t.check("e:nth-child(-n+1)", "e[not(preceding-sibling::*[1])]");
    t.check(
        "e:nth-child(-2n+2)",
        "e[not(preceding-sibling::*[2]) and (count(preceding-sibling::*) + 1) mod -2 = 0]",
    );
}

/// An element in any namespace has no node test for its type, which is
/// the expanded name: local-name() would count `a:e` and a no-namespace
/// `e` as one type. So every of-type pseudo-class refuses it, while an
/// `of S` count, which filters siblings by `S` rather than by type,
/// still translates.
#[test]
fn any_namespace_of_type() {
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        let mut t = Cases::new(mode);
        for css in [
            "*|e:first-of-type",
            "*|e:last-of-type",
            "*|e:nth-of-type(2)",
            "*|e:nth-last-of-type(2)",
        ] {
            assert_eq!(
                t.css_to_xpath(css, ""),
                Err(Error::Unsupported {
                    construct: "an of-type pseudo-class on an element in any namespace (`*|e`)"
                        .to_owned(),
                    offset: None,
                }),
                "{css} in {mode:?}"
            );
        }
        assert_eq!(
            t.css_to_xpath("*|e:only-of-type", ""),
            Err(Error::Unsupported {
                construct: "`:only-of-type` on an element in any namespace (`*|e`)".to_owned(),
                offset: None,
            }),
            "in {mode:?}"
        );
        t.check(
            "*|e:nth-child(2 of *|e)",
            "*[local-name() = 'e' and preceding-sibling::*[local-name() = 'e'][1] \
             and not(preceding-sibling::*[local-name() = 'e'][2])]",
        );
    }
}

/// `of S` selector lists (CSS Level 4), nth-child only.
#[test]
fn nth_child_of() {
    let mut t = Cases::new(Mode::Generic);
    t.check("div:nth-child(2 of .foo)", "div[contains(concat(' ', normalize-space(@class), ' '), ' foo ') and preceding-sibling::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')][1] and not(preceding-sibling::*[contains(concat(' ', normalize-space(@class), ' '), ' foo ')][2])]");
    // a=1, b<=1: only the current-element check remains.
    t.check(
        "li:nth-child(n of .item)",
        "li[contains(concat(' ', normalize-space(@class), ' '), ' item ')]",
    );
    // An impossible series never matches, whatever the `of` list says,
    // so the 0 absorbs the current-element check.
    t.check("li:nth-child(-n of .item)", "li[0]");
    // An element argument is a self:: test on the current element, and
    // the node test of the sibling step that counts.
    t.check("div:nth-child(2 of div.foo)", "div[contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::div and preceding-sibling::div[contains(concat(' ', normalize-space(@class), ' '), ' foo ')][1] and not(preceding-sibling::div[contains(concat(' ', normalize-space(@class), ' '), ' foo ')][2])]");
    // A universal argument makes the list match everything, like a
    // plain :nth-child.
    t.check(
        "li:nth-child(2 of .foo, *)",
        "li[preceding-sibling::*[1] and not(preceding-sibling::*[2])]",
    );
    // A chain argument that cannot match folds to 0 before the count()
    // test is built, so the dead chain is not emitted twice.
    t.check("li:nth-child(2 of a > b:hover)", "li[0]");
    // A list has no single name, so the siblings are filtered by the
    // whole check, and a name that is not a plain node test stays in
    // the predicate.
    t.check(
        "li:nth-child(2 of li, p)",
        "li[(self::li or self::p) and preceding-sibling::*[self::li or self::p][1] \
         and not(preceding-sibling::*[self::li or self::p][2])]",
    );
    t.check(
        "li:nth-child(odd of *|p)",
        "li[local-name() = 'p' and count(preceding-sibling::*[local-name() = 'p']) mod 2 = 0]",
    );
    // The sibling step's node test is folded as the mode folds names.
    let mut html = Cases::new(Mode::Html);
    html.check(
        "LI:nth-child(odd of LI)",
        "li[self::li and count(preceding-sibling::li) mod 2 = 0]",
    );
    let mut xhtml = Cases::new(Mode::Xhtml);
    xhtml.check(
        "LI:nth-child(odd of LI)",
        "LI[self::LI and count(preceding-sibling::LI) mod 2 = 0]",
    );
}
