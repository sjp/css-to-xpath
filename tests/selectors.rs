//! Type, universal, class, id and attribute selectors, and the four
//! combinators — the forms every other suite builds on.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

/// Type, namespace, and attribute selector forms.
#[test]
fn simple_selectors() {
    let mut t = Cases::new(Mode::Generic);
    t.check("*", "*");
    t.check("e", "e");
    t.check("*|e", "*[local-name() = 'e']");
    t.check("|e", "e");
    t.check("|*", "*[namespace-uri() = '']");
    t.check("*|*", "*");
    t.check("e|f", "e:f");
    t.check("svg|*", "svg:*");
    t.check("e[foo]", "e[@foo]");
    t.check("e[foo|bar]", "e[@foo:bar]");
    t.check("[*|foo]", "*[@*[local-name() = 'foo']]");
    t.check("[|foo]", "*[@foo]");
    t.check("ns|e", "ns:e");
    t.check("[ns|a]", "*[@ns:a]");
    t.check("[*|a='v']", "*[@*[local-name() = 'a'] = 'v']");
    t.check("e[foo=\"bar\"]", "e[@foo = 'bar']");
    t.check("e[foo=\"\"]", "e[@foo = '']");
    t.check("e[foo|=\"\"]", "e[@foo = '' or starts-with(@foo, '-')]");
    t.check(
        "e[foo~=\"bar\"]",
        "e[contains(concat(' ', normalize-space(@foo), ' '), ' bar ')]",
    );
    t.check("e[foo^=\"bar\"]", "e[starts-with(@foo, 'bar')]");
    t.check(
        "e[foo$=\"bar\"]",
        "e[substring(@foo, string-length(@foo) - 2) = 'bar']",
    );
    t.check("e[foo*=\"bar\"]", "e[contains(@foo, 'bar')]");
    t.check(
        "e[hreflang|=\"en\"]",
        "e[@hreflang = 'en' or starts-with(@hreflang, 'en-')]",
    );
    // Empty values can never satisfy substring/token operators.
    t.check("*[aval~=\"\"]", "*[0]");
    t.check("*[aval^=\"\"]", "*[0]");
    t.check("*[aval$=\"\"]", "*[0]");
    t.check("*[aval*=\"\"]", "*[0]");
    // Parenthesised / hex-digit-looking string content is not
    // mistaken for a unicode escape: it survives literally.
    t.check("e[foo='(test)']", "e[@foo = '(test)']");
    t.check("e[foo='(abc)']", "e[@foo = '(abc)']");
    t.check("e[foo='(e2e)']", "e[@foo = '(e2e)']");
    t.check("e[foo='(123)']", "e[@foo = '(123)']");
    t.check("e[foo='(12345)']", "e[@foo = '(12345)']");
    // Six hex digits is the max for a CSS unicode escape.
    t.check("e[foo='(abcdef)']", "e[@foo = '(abcdef)']");
    t.check("e[foo='(123456)']", "e[@foo = '(123456)']");
    // Seven hex digits exceeds the max, so no unicode escape applies.
    t.check("e[foo='(1234567)']", "e[@foo = '(1234567)']");
    t.check("e[foo='(AbCdEf)']", "e[@foo = '(AbCdEf)']");
    t.check("e[foo='(E2E)']", "e[@foo = '(E2E)']");
    t.check("e[foo='(o2o)']", "e[@foo = '(o2o)']");
    t.check("e[foo='(xyz)']", "e[@foo = '(xyz)']");
    t.check("e[foo='(test123)']", "e[@foo = '(test123)']");
    t.check("e[foo='(abc)(def)']", "e[@foo = '(abc)(def)']");
    t.check("e[foo='(abc )']", "e[@foo = '(abc )']");
}

#[test]
fn class_id_combinators() {
    let mut t = Cases::new(Mode::Generic);
    t.check(
        "e.warning",
        "e[contains(concat(' ', normalize-space(@class), ' '), ' warning ')]",
    );
    t.check("e#myid", "e[@id = 'myid']");
    t.check("e f", "e//f");
    t.check("e > f", "e/f");
    t.check("e + f", "e/following-sibling::*[1][self::f]");
    t.check("e ~ f", "e/following-sibling::f");
    t.check("e + f[bar]", "e/following-sibling::*[1][self::f][@bar]");
    // `+ *` needs no self:: test: the node test is already `*`, so
    // the [1] counts every sibling on its own.
    t.check("e + *", "e/following-sibling::*[1]");
    t.check("div#container p", "div[@id = 'container']//p");
    t.check("a , b", "a | b");
    // Namespaces on the '>' and '+' combinators' right-hand side.
    t.check("div > *|e", "div/*[local-name() = 'e']");
    t.check("e + |f", "e/following-sibling::*[1][self::f]");
    t.check("e + ns|f", "e/following-sibling::*[1][self::ns:f]");
    // `*|f` is already a `*` node test carrying a local-name()
    // condition, so the [1] counts every sibling with no self:: test
    // of its own.
    t.check("e + *|f", "e/following-sibling::*[1][local-name() = 'f']");
    // A compound stacks further simple selectors after the '+'
    // position test, in the order the CSS names them.
    t.check("a + b.test", "a/following-sibling::*[1][self::b][contains(concat(' ', normalize-space(@class), ' '), ' test ')]");
    t.check(
        "a + b#myid",
        "a/following-sibling::*[1][self::b][@id = 'myid']",
    );
    t.check(
        "a + b[id][title]",
        "a/following-sibling::*[1][self::b][@id and @title]",
    );
    t.check("a + b.test[title]", "a/following-sibling::*[1][self::b][contains(concat(' ', normalize-space(@class), ' '), ' test ') and @title]");
    t.check("a.link + b[id]", "a[contains(concat(' ', normalize-space(@class), ' '), ' link ')]/following-sibling::*[1][self::b][@id]");
    t.check("a[href] + b.test", "a[@href]/following-sibling::*[1][self::b][contains(concat(' ', normalize-space(@class), ' '), ' test ')]");
    t.check("div#main + p.intro[title]", "div[@id = 'main']/following-sibling::*[1][self::p][contains(concat(' ', normalize-space(@class), ' '), ' intro ') and @title]");
    t.check("h1 + *[rel=up]", "h1/following-sibling::*[1][@rel = 'up']");
    // A leading combinator chain applies '+' after the preceding step.
    t.check("div > h1 + p", "div/h1/following-sibling::*[1][self::p]");
    t.check(
        "div#main > h1 + p[class]",
        "div[@id = 'main']/h1/following-sibling::*[1][self::p][@class]",
    );
    t.check(
        "section a + b",
        "section//a/following-sibling::*[1][self::b]",
    );
    t.check("article.post > h2.title + p.intro[data-info]", "article[contains(concat(' ', normalize-space(@class), ' '), ' post ')]/h2[contains(concat(' ', normalize-space(@class), ' '), ' title ')]/following-sibling::*[1][self::p][contains(concat(' ', normalize-space(@class), ' '), ' intro ') and @data-info]");
    // '+' combines with the of-type pseudo family on the right-hand
    // side, testing the sibling's own preceding-sibling count.
    t.check(
        "h1 + p:first-child",
        "h1/following-sibling::*[1][self::p][count(preceding-sibling::*) = 0]",
    );
    t.check(
        "h1 + p:nth-child(2)",
        "h1/following-sibling::*[1][self::p][count(preceding-sibling::*) = 1]",
    );
}
