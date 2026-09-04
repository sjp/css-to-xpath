//! Element and attribute *names*: escapes, names that cannot be an
//! XPath name test, and what a missing namespace prefix means.

mod cases;
use cases::Cases;
use css_to_xpath::{Mode, Translator};

#[test]
fn unsafe_names_and_escapes() {
    let mut t = Cases::new(Mode::Generic);
    // A name that cannot be an XPath name test folds into a name()
    // comparison, pinned to the null namespace so that it means what
    // a name that can be one means; see
    // `unprefixed_names_mean_the_null_namespace_everywhere`.
    t.check("di\\[v", "*[name() = 'di[v' and namespace-uri() = '']");
    t.check("[h\\]ref]", "*[attribute::*[name() = 'h]ref']]");
    t.check(
        "di\u{a0}v",
        "*[name() = 'di\u{a0}v' and namespace-uri() = '']",
    );
    // Unicode escapes are decoded to the characters they represent,
    // in idents, hashes, and strings alike.
    t.check("#\\31 23", "*[@id = '123']");
    t.check("\\31 23", "*[name() = '123' and namespace-uri() = '']");
    t.check("[\\31 23]", "*[attribute::*[name() = '123']]");
    t.check("e[foo='\\31 23']", "e[@foo = '123']");
    t.check("e[foo='x\\79 z']", "e[@foo = 'xyz']");
    // A single hex digit is still a valid escape.
    t.check("e[foo='\\4a']", "e[@foo = 'J']");
    // An escaped backslash yields a literal backslash; what follows
    // it must not be re-processed as another escape.
    t.check("e[foo='x\\\\79 z']", "e[@foo = 'x\\79 z']");
    t.check("e[foo='\\\\31 23']", "e[@foo = '\\31 23']");
    t.check("#\\\\31 x", "*[@id = '\\31']//x");
    // '*|' bypasses the safe-name fallback: quoting handles it.
    t.check("*|di\\[v", "*[local-name() = 'di[v']");
    t.check("[*|h\\]ref]", "*[@*[local-name() = 'h]ref']]");
    // '|e' with a name needing quoting is the same translation: the
    // explicit no-namespace constraint is what an unprefixed name
    // already carries.
    t.check("|di\\[v", "*[name() = 'di[v' and namespace-uri() = '']");
    t.check("|é", "*[name() = 'é' and namespace-uri() = '']");
    // A prefix with a name needing quoting keeps the prefix in the
    // node test, so it still resolves through the caller's namespace
    // map, and compares only the local part.
    t.check("svg|di\\[v", "svg:*[local-name() = 'di[v']");
    t.check("svg|\\31 g", "svg:*[local-name() = '1g']");
    t.check("[svg|h\\]ref]", "*[@svg:*[local-name() = 'h]ref']]");
    t.check(
        "[svg|h\\]ref='v']",
        "*[@svg:*[local-name() = 'h]ref'] = 'v']",
    );
    t.check(
        "e:is(svg|di\\[v)",
        "e[local-name() = 'di[v' and self::svg:*]",
    );
    t.check(
        "e:has(> svg|di\\[v)",
        "e[child::svg:*[local-name() = 'di[v']]",
    );
    t.check(
        "e:has(+ svg|di\\[v)",
        "e[following-sibling::*[1][local-name() = 'di[v' and self::svg:*]]",
    );
    t.check(
        "e + svg|di\\[v",
        "e/following-sibling::*[1][self::svg:*][local-name() = 'di[v']",
    );
    // Unprefixed, the name test the '+' position predicate needs is a
    // name() comparison, so there is no self:: test to stack it after.
    t.check(
        "e + é",
        "e/following-sibling::*[1][name() = 'é' and namespace-uri() = '']",
    );
    // The of-type nodetest keeps both halves.
    t.check(
        "svg|di\\[v:first-of-type",
        "svg:*[local-name() = 'di[v' \
         and count(preceding-sibling::svg:*[local-name() = 'di[v']) = 0]",
    );
    // A prefix that is not an XPath name has no such fallback and
    // errors; see `unsupported_errors` in `errors.rs`.
}

/// A namespace prefix is held to the XML `NCName` production rather than
/// to the local name's stricter ASCII test. The looser rule is what the
/// prefix position needs: a prefix has no `local-name()` fallback to
/// approximate with, so anything the rule rejects is an error, and a
/// non-ASCII `NCName` is a perfectly good XPath name.
#[test]
fn non_ascii_namespace_prefixes() {
    let mut t = Cases::new(Mode::Generic);
    // The prefix goes into the node test as it stands, exactly as an
    // ASCII one does.
    t.check("nsé|div", "nsé:div");
    t.check("nsé|*", "nsé:*");
    t.check("[nsé|href]", "*[@nsé:href]");
    t.check("[nsé|href='v']", "*[@nsé:href = 'v']");
    t.check("中文|div", "中文:div");
    // A combining mark or an extender is a name character but not a
    // name *start*, so it may sit inside a prefix.
    t.check("a\u{b7}b|div", "a\u{b7}b:div");
    // The two rules compose: the prefix stays in the node test and the
    // local name still falls back to a local-name() comparison.
    t.check("nsé|é", "nsé:*[local-name() = 'é']");
    t.check("nsé|di\\[v", "nsé:*[local-name() = 'di[v']");
    // And they compose everywhere a prefixed name can be written.
    t.check("e:is(nsé|div)", "e[self::nsé:div]");
    t.check("e:has(> nsé|div)", "e[child::nsé:div]");
    t.check("e + nsé|div", "e/following-sibling::*[1][self::nsé:div]");
    t.check(
        "nsé|div:first-of-type",
        "nsé:div[count(preceding-sibling::nsé:div) = 0]",
    );
    // What the rule still rejects is a prefix that is not a name at
    // all; see `unsupported_errors` in `errors.rs`.
}

/// One policy for unprefixed type names wherever they appear: a name
/// written without a prefix means the null namespace, and `*|e` is
/// the escape hatch for "this name in any namespace". Inside a
/// functional pseudo-class argument that means a `self::` test, never
/// a bare `name()` comparison: `name()` returns the *qualified* name,
/// so it also matches the name in a *default* namespace (XHTML, SVG,
/// Atom, ...), and `:is(p)` would then select what `p` does not.
#[test]
fn unprefixed_names_mean_the_null_namespace_everywhere() {
    let mut t = Cases::new(Mode::Generic);
    // Top level, and the right-hand side of every combinator.
    t.check("p", "p");
    t.check("|p", "p");
    t.check("body > p", "body/p");
    t.check("p ~ p", "p/following-sibling::p");
    t.check("p + p", "p/following-sibling::*[1][self::p]");
    // The same name inside an argument, as a `self::` test.
    t.check(":is(p)", "*[self::p]");
    t.check(":where(p)", "*[self::p]");
    t.check(":not(p)", "*[not(self::p)]");
    t.check(":is(body > p)", "*[self::p and parent::*[self::body]]");
    t.check(
        ":is(p + p)",
        "*[self::p and preceding-sibling::*[1][self::p]]",
    );
    t.check(
        "e:nth-child(1 of p)",
        "e[count(preceding-sibling::*[self::p]) = 0 and self::p]",
    );
    // :has() looks forward, so the name stays in the node test of the
    // existence path — except under `+`, where the [1] position
    // predicate has to count every sibling.
    t.check(":has(p)", "*[.//p]");
    t.check(":has(> p)", "*[child::p]");
    t.check("e:has(+ p)", "e[following-sibling::*[1][self::p]]");

    // A name needing quoting cannot be a node test at all, so it
    // folds into a name() comparison — which compares the qualified
    // name, and so needs the namespace pinned to mean the same as a
    // name that can.
    const E: &str = "name() = 'é' and namespace-uri() = ''";
    t.check("é", format!("*[{E}]"));
    t.check("|é", format!("*[{E}]"));
    t.check("é ~ é", format!("*[{E}]/following-sibling::*[{E}]"));
    t.check(":is(é)", format!("*[{E}]"));
    t.check("e:is(é > é)", format!("e[{E} and parent::*[{E}]]"));
    t.check("e:has(é)", format!("e[.//*[{E}]]"));
    t.check("e:has(+ é)", format!("e[following-sibling::*[1][{E}]]"));
    t.check(
        "e:nth-child(1 of é)",
        format!("e[count(preceding-sibling::*[{E}]) = 0 and {E}]"),
    );

    // `*|e` asks for the name in any namespace, and it too means the
    // same thing wherever it is written.
    t.check("*|p", "*[local-name() = 'p']");
    t.check(":is(*|p)", "*[local-name() = 'p']");
    t.check(":has(*|p)", "*[.//*[local-name() = 'p']]");
    t.check(
        ":is(*|body > *|p)",
        "*[local-name() = 'p' and parent::*[local-name() = 'body']]",
    );
}

/// A default namespace prefix qualifies exactly what CSS Namespaces 3
/// says it does: type selectors, and the implicit universal selector of
/// a compound that has none. `|e` and `*|e` keep their meanings, and
/// attribute names — which have no namespace unless one is written —
/// are untouched.
#[test]
fn default_namespace_qualifies_unprefixed_type_selectors() {
    let mut t = Cases::with_translator(
        Translator::new(Mode::Xhtml).with_default_namespace_prefix("h"),
        "",
    );

    // A type selector, wherever it appears — the same policy the
    // unprefixed case follows, one step along.
    t.check("p", "h:p");
    t.check("body > p", "h:body/h:p");
    t.check("p + p", "h:p/following-sibling::*[1][self::h:p]");
    t.check("p:is(a, b)", "h:p[self::h:a or self::h:b]");
    t.check("p:not(a)", "h:p[not(self::h:a)]");
    t.check("p:has(> a)", "h:p[child::h:a]");
    t.check(
        "e:nth-child(1 of p)",
        "h:e[count(preceding-sibling::*[self::h:p]) = 0 and self::h:p]",
    );
    // The of-type family counts by the qualified node test, so the
    // prefix reaches the sibling count too.
    t.check("p:nth-of-type(2)", "h:p[count(preceding-sibling::h:p) = 1]");

    // The implicit universal of a type-less compound, including the
    // written `*` and `:scope`.
    t.check(
        ".c",
        "h:*[contains(concat(' ', normalize-space(@class), ' '), ' c ')]",
    );
    t.check("*", "h:*");
    t.check(":scope > a", "self::h:*/h:a");
    // ... but not the subject of an `:is()` / `:where()` / `:not()`
    // argument that has no type selector of its own: Selectors 4 makes
    // those compounds featureless.
    t.check(
        "p:is(.c)",
        "h:p[contains(concat(' ', normalize-space(@class), ' '), ' c ')]",
    );

    // The written namespace forms are unaffected. A prefix naming the
    // default namespace is the default namespace, so `h|p` and `p` are
    // one selector.
    t.check("h|p", "h:p");
    t.check("x|p", "x:p");
    t.check("|p", "p");
    t.check("|*", "*[namespace-uri() = '']");
    t.check("*|p", "*[local-name() = 'p']");
    t.check("*|*", "*");

    // Attribute names have no namespace unless one is written, so the
    // default namespace never reaches them.
    t.check("[foo]", "h:*[@foo]");
    t.check("[|foo]", "h:*[@foo]");
    t.check("[x|foo]", "h:*[@x:foo]");
    t.check("[*|foo]", "h:*[@*[local-name() = 'foo']]");

    // A name that cannot be a node test keeps the prefix in the node
    // test and compares the local part, exactly as a written prefix
    // does — the qualified-name comparison an unprefixed name folds
    // into would not resolve through the caller's namespace map.
    t.check("é", "h:*[local-name() = 'é']");

    // The HTML overrides identify elements by local-name(), so they are
    // namespace-agnostic and read the same under a default namespace.
    t.check(
        "input:checked",
        "h:input[@checked and (translate(@type, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz') = 'checkbox' or translate(@type, \
         'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz') = 'radio')]",
    );
}

/// The prefix a default namespace is spelled with is checked exactly as
/// a written one is, and an empty prefix means no default namespace at
/// all — the state a translator starts in.
#[test]
fn default_namespace_prefix_is_checked_like_a_written_one() {
    let unsafe_prefix = Translator::new(Mode::Generic).with_default_namespace_prefix("1x");
    let err = unsafe_prefix.css_to_xpath("p", "").unwrap_err();
    assert_eq!(
        err.to_string(),
        "unsupported CSS construct: a namespace prefix that is not an XPath name (`1x`)"
    );

    let empty = Translator::new(Mode::Generic).with_default_namespace_prefix("");
    assert_eq!(empty.css_to_xpath("p", "").unwrap(), "p");
    assert_eq!(
        empty.css_to_xpath("*|p", "").unwrap(),
        "*[local-name() = 'p']"
    );
}
