mod parser;
mod translate;

pub use translate::{Error, Mode, Translator};

/// The version of this crate, from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Translate a CSS selector to an XPath 1.0 expression.
///
/// # Arguments
///
/// * `css` — A CSS selector string.
/// * `prefix` — An XPath path prefix prepended to the result
///   (e.g. `"descendant-or-self::"`).  Pass `""` for none.
/// * `mode` — The translator flavour: [`Mode::Generic`], [`Mode::Html`], or
///   [`Mode::Xhtml`].
///
/// # Errors
///
/// Returns an [`Error`] when the selector is syntactically invalid or uses
/// an unsupported construct.
pub fn css_to_xpath(css: &str, prefix: &str, mode: Mode) -> Result<String, Error> {
    Translator::new(mode).css_to_xpath(css, prefix)
}

#[cfg(test)]
mod tests {
    use crate::translate::{Mode, Translator};

    fn xpath(css: &str) -> String {
        Translator::new(Mode::Generic)
            .css_to_xpath(css, "")
            .unwrap()
    }

    /// Type, namespace, and attribute selector forms.
    #[test]
    fn simple_selectors() {
        assert_eq!(xpath("*"), "*");
        assert_eq!(xpath("e"), "e");
        assert_eq!(xpath("*|e"), "*[local-name() = 'e']");
        assert_eq!(xpath("|e"), "e");
        assert_eq!(xpath("|*"), "*[namespace-uri() = '']");
        assert_eq!(xpath("*|*"), "*");
        assert_eq!(xpath("e|f"), "e:f");
        assert_eq!(xpath("svg|*"), "svg:*");
        assert_eq!(xpath("e[foo]"), "e[@foo]");
        assert_eq!(xpath("e[foo|bar]"), "e[@foo:bar]");
        assert_eq!(xpath("[*|foo]"), "*[@*[local-name() = 'foo']]");
        assert_eq!(xpath("[|foo]"), "*[@foo]");
        assert_eq!(xpath("ns|e"), "ns:e");
        assert_eq!(xpath("[ns|a]"), "*[@ns:a]");
        assert_eq!(xpath("[*|a='v']"), "*[@*[local-name() = 'a'] = 'v']");
        assert_eq!(xpath("e[foo=\"bar\"]"), "e[@foo = 'bar']");
        assert_eq!(xpath("e[foo=\"\"]"), "e[@foo = '']");
        assert_eq!(
            xpath("e[foo|=\"\"]"),
            "e[@foo and (@foo = '' or starts-with(@foo, '-'))]"
        );
        assert_eq!(
            xpath("e[foo~=\"bar\"]"),
            "e[@foo and contains(concat(' ', normalize-space(@foo), ' '), ' bar ')]"
        );
        assert_eq!(
            xpath("e[foo^=\"bar\"]"),
            "e[@foo and starts-with(@foo, 'bar')]"
        );
        assert_eq!(
            xpath("e[foo$=\"bar\"]"),
            "e[@foo and substring(@foo, string-length(@foo)-2) = 'bar']"
        );
        assert_eq!(
            xpath("e[foo*=\"bar\"]"),
            "e[@foo and contains(@foo, 'bar')]"
        );
        assert_eq!(
            xpath("e[hreflang|=\"en\"]"),
            "e[@hreflang and (@hreflang = 'en' or starts-with(@hreflang, 'en-'))]"
        );
        // Empty values can never satisfy substring/token operators.
        assert_eq!(xpath("*[aval~=\"\"]"), "*[0]");
        assert_eq!(xpath("*[aval^=\"\"]"), "*[0]");
        assert_eq!(xpath("*[aval$=\"\"]"), "*[0]");
        assert_eq!(xpath("*[aval*=\"\"]"), "*[0]");
        // Parenthesised / hex-digit-looking string content is not
        // mistaken for a unicode escape: it survives literally.
        assert_eq!(xpath("e[foo='(test)']"), "e[@foo = '(test)']");
        assert_eq!(xpath("e[foo='(abc)']"), "e[@foo = '(abc)']");
        assert_eq!(xpath("e[foo='(e2e)']"), "e[@foo = '(e2e)']");
        assert_eq!(xpath("e[foo='(123)']"), "e[@foo = '(123)']");
        assert_eq!(xpath("e[foo='(12345)']"), "e[@foo = '(12345)']");
        // Six hex digits is the max for a CSS unicode escape.
        assert_eq!(xpath("e[foo='(abcdef)']"), "e[@foo = '(abcdef)']");
        assert_eq!(xpath("e[foo='(123456)']"), "e[@foo = '(123456)']");
        // Seven hex digits exceeds the max, so no unicode escape applies.
        assert_eq!(xpath("e[foo='(1234567)']"), "e[@foo = '(1234567)']");
        assert_eq!(xpath("e[foo='(AbCdEf)']"), "e[@foo = '(AbCdEf)']");
        assert_eq!(xpath("e[foo='(E2E)']"), "e[@foo = '(E2E)']");
        assert_eq!(xpath("e[foo='(o2o)']"), "e[@foo = '(o2o)']");
        assert_eq!(xpath("e[foo='(xyz)']"), "e[@foo = '(xyz)']");
        assert_eq!(xpath("e[foo='(test123)']"), "e[@foo = '(test123)']");
        assert_eq!(xpath("e[foo='(abc)(def)']"), "e[@foo = '(abc)(def)']");
        assert_eq!(xpath("e[foo='(abc )']"), "e[@foo = '(abc )']");
    }

    #[test]
    fn class_id_combinators() {
        assert_eq!(
            xpath("e.warning"),
            "e[@class and contains(concat(' ', normalize-space(@class), ' '), ' warning ')]"
        );
        assert_eq!(xpath("e#myid"), "e[@id = 'myid']");
        assert_eq!(xpath("e f"), "e//f");
        assert_eq!(xpath("e > f"), "e/f");
        assert_eq!(xpath("e + f"), "e/following-sibling::*[1][self::f]");
        assert_eq!(xpath("e ~ f"), "e/following-sibling::f");
        assert_eq!(
            xpath("e + f[bar]"),
            "e/following-sibling::*[1][self::f][@bar]"
        );
        assert_eq!(xpath("e + *"), "e/following-sibling::*[1][self::*]");
        assert_eq!(xpath("div#container p"), "div[@id = 'container']//p");
        assert_eq!(xpath("a , b"), "a | b");
        // Namespaces on the '>' and '+' combinators' right-hand side.
        assert_eq!(xpath("div > *|e"), "div/*[local-name() = 'e']");
        assert_eq!(xpath("e + |f"), "e/following-sibling::*[1][self::f]");
        assert_eq!(xpath("e + ns|f"), "e/following-sibling::*[1][self::ns:f]");
        assert_eq!(
            xpath("e + *|f"),
            "e/following-sibling::*[1][self::*][local-name() = 'f']"
        );
        // A compound stacks further simple selectors after the '+'
        // position test, in the order the CSS names them.
        assert_eq!(
            xpath("a + b.test"),
            "a/following-sibling::*[1][self::b][@class and contains(concat(' ', normalize-space(@class), ' '), ' test ')]"
        );
        assert_eq!(
            xpath("a + b#myid"),
            "a/following-sibling::*[1][self::b][@id = 'myid']"
        );
        assert_eq!(
            xpath("a + b[id][title]"),
            "a/following-sibling::*[1][self::b][@id and @title]"
        );
        assert_eq!(
            xpath("a + b.test[title]"),
            "a/following-sibling::*[1][self::b][@class and contains(concat(' ', normalize-space(@class), ' '), ' test ') and @title]"
        );
        assert_eq!(
            xpath("a.link + b[id]"),
            "a[@class and contains(concat(' ', normalize-space(@class), ' '), ' link ')]/following-sibling::*[1][self::b][@id]"
        );
        assert_eq!(
            xpath("a[href] + b.test"),
            "a[@href]/following-sibling::*[1][self::b][@class and contains(concat(' ', normalize-space(@class), ' '), ' test ')]"
        );
        assert_eq!(
            xpath("div#main + p.intro[title]"),
            "div[@id = 'main']/following-sibling::*[1][self::p][@class and contains(concat(' ', normalize-space(@class), ' '), ' intro ') and @title]"
        );
        assert_eq!(
            xpath("h1 + *[rel=up]"),
            "h1/following-sibling::*[1][self::*][@rel = 'up']"
        );
        // A leading combinator chain applies '+' after the preceding step.
        assert_eq!(
            xpath("div > h1 + p"),
            "div/h1/following-sibling::*[1][self::p]"
        );
        assert_eq!(
            xpath("div#main > h1 + p[class]"),
            "div[@id = 'main']/h1/following-sibling::*[1][self::p][@class]"
        );
        assert_eq!(
            xpath("section a + b"),
            "section//a/following-sibling::*[1][self::b]"
        );
        assert_eq!(
            xpath("article.post > h2.title + p.intro[data-info]"),
            "article[@class and contains(concat(' ', normalize-space(@class), ' '), ' post ')]/h2[@class and contains(concat(' ', normalize-space(@class), ' '), ' title ')]/following-sibling::*[1][self::p][@class and contains(concat(' ', normalize-space(@class), ' '), ' intro ') and @data-info]"
        );
        // '+' combines with the of-type pseudo family on the right-hand
        // side, testing the sibling's own preceding-sibling count.
        assert_eq!(
            xpath("h1 + p:first-child"),
            "h1/following-sibling::*[1][self::p][count(preceding-sibling::*) = 0]"
        );
        assert_eq!(
            xpath("h1 + p:nth-child(2)"),
            "h1/following-sibling::*[1][self::p][count(preceding-sibling::*) = 1]"
        );
    }

    #[test]
    fn unsafe_names_and_escapes() {
        // A name that cannot be an XPath name test folds into a name()
        // comparison, pinned to the null namespace so that it means what
        // a name that can be one means; see
        // `unprefixed_names_mean_the_null_namespace_everywhere`.
        assert_eq!(
            xpath("di\\[v"),
            "*[name() = 'di[v' and namespace-uri() = '']"
        );
        assert_eq!(xpath("[h\\]ref]"), "*[attribute::*[name() = 'h]ref']]");
        assert_eq!(
            xpath("di\u{a0}v"),
            "*[name() = 'di\u{a0}v' and namespace-uri() = '']"
        );
        // Unicode escapes are decoded to the characters they represent,
        // in idents, hashes, and strings alike.
        assert_eq!(xpath("#\\31 23"), "*[@id = '123']");
        assert_eq!(
            xpath("\\31 23"),
            "*[name() = '123' and namespace-uri() = '']"
        );
        assert_eq!(xpath("[\\31 23]"), "*[attribute::*[name() = '123']]");
        assert_eq!(xpath("e[foo='\\31 23']"), "e[@foo = '123']");
        assert_eq!(xpath("e[foo='x\\79 z']"), "e[@foo = 'xyz']");
        // A single hex digit is still a valid escape.
        assert_eq!(xpath("e[foo='\\4a']"), "e[@foo = 'J']");
        // An escaped backslash yields a literal backslash; what follows
        // it must not be re-processed as another escape.
        assert_eq!(xpath("e[foo='x\\\\79 z']"), "e[@foo = 'x\\79 z']");
        assert_eq!(xpath("e[foo='\\\\31 23']"), "e[@foo = '\\31 23']");
        assert_eq!(xpath("#\\\\31 x"), "*[@id = '\\31']//x");
        // '*|' bypasses the safe-name fallback: quoting handles it.
        assert_eq!(xpath("*|di\\[v"), "*[local-name() = 'di[v']");
        assert_eq!(xpath("[*|h\\]ref]"), "*[@*[local-name() = 'h]ref']]");
        // '|e' with a name needing quoting is the same translation: the
        // explicit no-namespace constraint is what an unprefixed name
        // already carries.
        assert_eq!(
            xpath("|di\\[v"),
            "*[name() = 'di[v' and namespace-uri() = '']"
        );
        assert_eq!(xpath("|é"), "*[name() = 'é' and namespace-uri() = '']");
        // A prefix with a name needing quoting keeps the prefix in the
        // node test, so it still resolves through the caller's namespace
        // map, and compares only the local part.
        assert_eq!(xpath("svg|di\\[v"), "svg:*[local-name() = 'di[v']");
        assert_eq!(xpath("svg|\\31 g"), "svg:*[local-name() = '1g']");
        assert_eq!(xpath("[svg|h\\]ref]"), "*[@svg:*[local-name() = 'h]ref']]");
        assert_eq!(
            xpath("[svg|h\\]ref='v']"),
            "*[@svg:*[local-name() = 'h]ref'] = 'v']"
        );
        assert_eq!(
            xpath("e:is(svg|di\\[v)"),
            "e[local-name() = 'di[v' and self::svg:*]"
        );
        assert_eq!(
            xpath("e:has(> svg|di\\[v)"),
            "e[child::svg:*[local-name() = 'di[v']]"
        );
        assert_eq!(
            xpath("e:has(+ svg|di\\[v)"),
            "e[following-sibling::*[1][local-name() = 'di[v' and self::svg:*]]"
        );
        assert_eq!(
            xpath("e + svg|di\\[v"),
            "e/following-sibling::*[1][self::svg:*][local-name() = 'di[v']"
        );
        // The of-type nodetest keeps both halves.
        assert_eq!(
            xpath("svg|di\\[v:first-of-type"),
            "svg:*[local-name() = 'di[v' \
             and count(preceding-sibling::svg:*[local-name() = 'di[v']) = 0]"
        );
        // A prefix that itself needs quoting errors; see
        // `unsupported_errors`.
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
        // Top level, and the right-hand side of every combinator.
        assert_eq!(xpath("p"), "p");
        assert_eq!(xpath("|p"), "p");
        assert_eq!(xpath("body > p"), "body/p");
        assert_eq!(xpath("p ~ p"), "p/following-sibling::p");
        assert_eq!(xpath("p + p"), "p/following-sibling::*[1][self::p]");
        // The same name inside an argument, as a `self::` test.
        assert_eq!(xpath(":is(p)"), "*[self::p]");
        assert_eq!(xpath(":where(p)"), "*[self::p]");
        assert_eq!(xpath(":not(p)"), "*[not(self::p)]");
        assert_eq!(
            xpath(":is(body > p)"),
            "*[self::p and parent::*[self::body]]"
        );
        assert_eq!(
            xpath(":is(p + p)"),
            "*[self::p and preceding-sibling::*[1][self::p]]"
        );
        assert_eq!(
            xpath("e:nth-child(1 of p)"),
            "e[count(preceding-sibling::*[self::p]) = 0 and self::p]"
        );
        // :has() looks forward, so the name stays in the node test of the
        // existence path — except under `+`, where the [1] position
        // predicate has to count every sibling.
        assert_eq!(xpath(":has(p)"), "*[.//p]");
        assert_eq!(xpath(":has(> p)"), "*[child::p]");
        assert_eq!(xpath("e:has(+ p)"), "e[following-sibling::*[1][self::p]]");

        // A name needing quoting cannot be a node test at all, so it
        // folds into a name() comparison — which compares the qualified
        // name, and so needs the namespace pinned to mean the same as a
        // name that can.
        const E: &str = "name() = 'é' and namespace-uri() = ''";
        assert_eq!(xpath("é"), format!("*[{E}]"));
        assert_eq!(xpath("|é"), format!("*[{E}]"));
        assert_eq!(xpath("é ~ é"), format!("*[{E}]/following-sibling::*[{E}]"));
        assert_eq!(xpath(":is(é)"), format!("*[{E}]"));
        assert_eq!(xpath("e:is(é > é)"), format!("e[{E} and parent::*[{E}]]"));
        assert_eq!(xpath("e:has(é)"), format!("e[.//*[{E}]]"));
        assert_eq!(
            xpath("e:has(+ é)"),
            format!("e[following-sibling::*[1][{E}]]")
        );
        assert_eq!(
            xpath("e:nth-child(1 of é)"),
            format!("e[count(preceding-sibling::*[{E}]) = 0 and {E}]")
        );

        // `*|e` asks for the name in any namespace, and it too means the
        // same thing wherever it is written.
        assert_eq!(xpath("*|p"), "*[local-name() = 'p']");
        assert_eq!(xpath(":is(*|p)"), "*[local-name() = 'p']");
        assert_eq!(xpath(":has(*|p)"), "*[.//*[local-name() = 'p']]");
        assert_eq!(
            xpath(":is(*|body > *|p)"),
            "*[local-name() = 'p' and parent::*[local-name() = 'body']]"
        );
    }

    #[test]
    fn case_sensitivity_flags() {
        const LOWER_FOO: &str = "translate(@foo, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                                 'abcdefghijklmnopqrstuvwxyz')";
        assert_eq!(xpath("e[foo=\"Bar\" i]"), format!("e[{LOWER_FOO} = 'bar']"));
        // Flag idents are themselves case-insensitive.
        assert_eq!(xpath("e[foo=\"Bar\" I]"), format!("e[{LOWER_FOO} = 'bar']"));
        assert_eq!(
            xpath("e[foo^=\"Bar\" i]"),
            format!("e[{LOWER_FOO} and starts-with({LOWER_FOO}, 'bar')]")
        );
        assert_eq!(
            xpath("e[foo$=\"Bar\" i]"),
            format!(
                "e[{LOWER_FOO} and substring({LOWER_FOO}, \
                 string-length({LOWER_FOO})-2) = 'bar']"
            )
        );
        assert_eq!(
            xpath("e[foo*=\"Bar\" i]"),
            format!("e[{LOWER_FOO} and contains({LOWER_FOO}, 'bar')]")
        );
        assert_eq!(
            xpath("e[foo~=\"Bar\" i]"),
            format!(
                "e[{LOWER_FOO} and contains(concat(' ', \
                 normalize-space({LOWER_FOO}), ' '), ' bar ')]"
            )
        );
        assert_eq!(
            xpath("e[foo|=\"Bar\" i]"),
            format!(
                "e[{LOWER_FOO} and ({LOWER_FOO} = 'bar' or \
                 starts-with({LOWER_FOO}, 'bar-'))]"
            )
        );
        // 's' requests default case-sensitive matching on any operator.
        assert_eq!(
            xpath("e[foo^=\"Bar\" s]"),
            "e[@foo and starts-with(@foo, 'Bar')]"
        );
        // ASCII-only lowering: non-ASCII characters are left alone.
        assert_eq!(
            xpath("e[foo=\"B\u{e4}r\" i]"),
            format!("e[{LOWER_FOO} = 'b\u{e4}r']")
        );
        // An empty value keeps the exact translation.
        assert_eq!(xpath("e[foo=\"\" i]"), "e[@foo = '']");
        // 's' requests the default case-sensitive matching.
        assert_eq!(xpath("e[foo=\"Bar\" s]"), "e[@foo = 'Bar']");
        // The flag composes with namespaced attribute forms.
        assert_eq!(
            xpath("e[*|foo=\"Bar\" i]"),
            "e[translate(@*[local-name() = 'foo'], \
             'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
             'abcdefghijklmnopqrstuvwxyz') = 'bar']"
        );
    }

    /// Attribute values containing quote characters pick a delimiter that
    /// avoids escaping, falling back to per-character `concat(...)` when
    /// the value contains both.
    #[test]
    fn quote_escaping() {
        // A value with only apostrophes is wrapped in double quotes.
        assert_eq!(xpath("*[aval=\"'\"]"), "*[@aval = \"'\"]");
        assert_eq!(xpath("*[aval=\"'''\"]"), "*[@aval = \"'''\"]");
        // A value with only double quotes is wrapped in single quotes.
        assert_eq!(xpath("*[aval='\"']"), "*[@aval = '\"']");
        assert_eq!(xpath("*[aval='\"\"\"']"), "*[@aval = '\"\"\"']");
        // A value with both falls back to concat(), one literal per char.
        assert_eq!(
            xpath("*[aval='\"\\'\"']"),
            "*[@aval = concat('\"',\"'\",'\"')]"
        );
    }

    /// Nesting depth is bounded before Servo is entered, so neither the
    /// parser, the translator, nor dropping the selector tree can recurse
    /// far enough to overflow the stack — an overflow aborts the process
    /// outright, which no caller can catch. The whole test runs on a
    /// thread with the 2 MB stack the limit is sized against (Rust's
    /// default for a spawned thread), so a regression fails the build
    /// instead of tearing the test runner down.
    #[test]
    fn nesting_depth_is_bounded() {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(|| {
                let t = Translator::new(Mode::Generic);
                let nest = |open: &str, n: usize| format!("{}a{}", open.repeat(n), ")".repeat(n));
                let too_deep = crate::Error::Unsupported(
                    "functional pseudo-classes nested more than 64 levels deep".to_owned(),
                );

                for open in [":not(", ":is(", ":where(", ":matches("] {
                    assert!(t.css_to_xpath(&nest(open, 64), "").is_ok());
                    assert_eq!(t.css_to_xpath(&nest(open, 65), "").unwrap_err(), too_deep);
                    // Well past the depth that used to abort the process.
                    assert_eq!(
                        t.css_to_xpath(&nest(open, 10_000), "").unwrap_err(),
                        too_deep
                    );
                }
                assert_eq!(
                    too_deep.into_message(":not(:not(a))"),
                    "The CSS selector \":not(:not(a))\" uses functional pseudo-classes \
                     nested more than 64 levels deep, which this translator does not support"
                );

                // Parens inside strings, escapes, and comments are not
                // nesting, exactly as the `||` scan treats pipes.
                let parens = "(".repeat(1000);
                assert!(t.css_to_xpath(&format!("[foo=\"{parens}\"]"), "").is_ok());
                assert!(t.css_to_xpath(&format!("a /* {parens} */ b"), "").is_ok());
                assert!(t.css_to_xpath(&"a\\(".repeat(1000), "").is_ok());

                // Depth is the limit, not length: an argument chain adds
                // no stack frames, so a chain far longer than the depth
                // limit still translates.
                let chain = vec!["a"; 20_000].join(" > ");
                assert!(t.css_to_xpath(&format!(":is({chain})"), "").is_ok());
                assert!(t.css_to_xpath(&format!("b:has({chain})"), "").is_ok());
                assert!(t.css_to_xpath(&chain, "").is_ok());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Length is linear, not quadratic: every combinator appends to the
    /// accumulated path instead of re-rendering it, so a chain far longer
    /// than anything a person would write still translates promptly. The
    /// assertions are on the output, not the clock — a regression to
    /// re-rendering shows up as the test taking minutes.
    #[test]
    fn long_chains_translate() {
        let t = Translator::new(Mode::Generic);
        let chain = vec!["a"; 100_000].join(" > ");
        let xpath = t.css_to_xpath(&chain, "//").unwrap();
        assert_eq!(xpath, format!("//{}", vec!["a"; 100_000].join("/")));

        // The same chain as an argument nests one existence test per
        // compound, which the wrapping must likewise build in one pass.
        let inner = t.css_to_xpath(&format!("b:is({chain})"), "//").unwrap();
        assert_eq!(
            inner,
            format!(
                "//b[{}self::a{}]",
                "self::a and parent::*[".repeat(99_999),
                "]".repeat(99_999)
            )
        );
    }

    #[test]
    fn unsupported_errors() {
        let t = Translator::new(Mode::Generic);
        // The non-standard [a!=b] and :contains() are not supported.
        assert!(t.css_to_xpath("e[foo!=\"bar\"]", "").is_err());
        assert!(t.css_to_xpath("e:contains(\"foo\")", "").is_err());
        assert!(t.css_to_xpath("e::before", "").is_err());
        assert!(t.css_to_xpath("e:", "").is_err());
        assert!(t.css_to_xpath("", "").is_err());
        // A flag requires an operator and value.
        assert!(t.css_to_xpath("[rel i]", "").is_err());
        assert!(t.css_to_xpath("[rel=stylesheet k]", "").is_err());
        assert!(t.css_to_xpath("[rel=stylesheet i i]", "").is_err());
        // Unknown pseudo-classes error.
        assert!(t.css_to_xpath("e:unknown-pseudo", "").is_err());
        assert!(t.css_to_xpath("e:first-line", "").is_err()); // pseudo-element
        // The Level 4 column combinator and grid-structural pseudos have
        // no XPath 1.0 translation: column membership rests on
        // colspan/rowspan layout arithmetic. `||` is caught before Servo
        // misparses it as namespace syntax...
        assert!(t.css_to_xpath("col || td", "").is_err());
        assert!(t.css_to_xpath("col||td", "").is_err());
        assert!(t.css_to_xpath("e:nth-col(2)", "").is_err());
        assert!(t.css_to_xpath("e:nth-last-col(2n)", "").is_err());
        // ...while pipes in strings, escapes, and comments stay valid.
        assert!(t.css_to_xpath("[foo=\"a||b\"]", "").is_ok());
        assert!(t.css_to_xpath("a\\|\\|b", "").is_ok());
        assert!(t.css_to_xpath("a /* || */ b", "").is_ok());
        // Pseudo-classes outside the never-match policy (see PseudoClass)
        // error rather than silently matching nothing: form validity and
        // state could be at least partially translated some day, and
        // erroring keeps typos loud.
        assert!(t.css_to_xpath("e:valid", "").is_err());
        assert!(t.css_to_xpath("e:user-invalid", "").is_err());
        assert!(t.css_to_xpath("e:read-only", "").is_err());
        assert!(t.css_to_xpath("e:placeholder-shown", "").is_err());
        assert!(t.css_to_xpath("e:defined", "").is_err());
        // :scope is supported in the leftmost compound only, and never
        // inside functional pseudo-class arguments (the context node is
        // unreachable from an XPath 1.0 predicate).
        assert!(t.css_to_xpath("a :scope", "").is_err());
        assert!(t.css_to_xpath("a > :scope", "").is_err());
        assert!(t.css_to_xpath(":scope :scope", "").is_err());
        // Inside a functional pseudo-class, the context node is
        // unreachable from an XPath 1.0 predicate: all four entry points
        // hit the same `describe_component` message.
        let scope_in_functional = crate::Error::Unsupported(
            "the `:scope` pseudo-class inside a functional pseudo-class".to_owned(),
        );
        assert_eq!(
            t.css_to_xpath("e:is(:scope)", "").unwrap_err(),
            scope_in_functional
        );
        assert_eq!(
            t.css_to_xpath("e:not(:scope)", "").unwrap_err(),
            scope_in_functional
        );
        assert_eq!(
            t.css_to_xpath("e:has(:scope)", "").unwrap_err(),
            scope_in_functional
        );
        assert_eq!(
            t.css_to_xpath("e:nth-child(2 of :scope)", "").unwrap_err(),
            scope_in_functional
        );
        // A leading combinator is :has()-only; dangling and doubled
        // combinators are parse errors everywhere.
        assert!(t.css_to_xpath("e:is(> a)", "").is_err());
        assert!(t.css_to_xpath("e:has(> > a)", "").is_err());
        assert!(t.css_to_xpath("e:has(>)", "").is_err());
        assert!(t.css_to_xpath("e:has(a >)", "").is_err());
        // Nested :has() is rejected (selectors-4).
        assert!(t.css_to_xpath("e:has(a:has(b))", "").is_err());
        assert!(t.css_to_xpath("e:has(> a:has(b))", "").is_err());
        // of-type pseudos are not implemented on `*` — including compounds
        // that leave the type implicit (`.foo` is `*.foo`) or carry it
        // only inside a pseudo-class argument. XPath 1.0 cannot compare a
        // sibling's name with the matched element's own name, so only a
        // type named in the compound itself gives a sibling node test.
        assert!(t.css_to_xpath("*:first-of-type", "").is_err());
        assert!(t.css_to_xpath("*:last-of-type", "").is_err());
        assert!(t.css_to_xpath("*:nth-of-type(2n)", "").is_err());
        assert!(t.css_to_xpath("*:nth-last-of-type(2)", "").is_err());
        assert!(t.css_to_xpath("*:only-of-type", "").is_err());
        assert!(t.css_to_xpath(".foo:first-of-type", "").is_err());
        assert!(t.css_to_xpath("[bar]:nth-of-type(2)", "").is_err());
        assert!(t.css_to_xpath(":is(e):first-of-type", "").is_err());
        // Every wildcard subject is universal for this purpose: a
        // prefixed wildcard names a namespace, not a type, so counting
        // `ns|*` siblings would be a position among all elements in that
        // namespace rather than among elements of the same type.
        assert!(t.css_to_xpath("svg|*:first-of-type", "").is_err());
        assert!(t.css_to_xpath("svg|*:last-of-type", "").is_err());
        assert!(t.css_to_xpath("svg|*:nth-of-type(2)", "").is_err());
        assert!(t.css_to_xpath("svg|*:nth-last-of-type(2)", "").is_err());
        assert!(t.css_to_xpath("svg|*:only-of-type", "").is_err());
        assert!(t.css_to_xpath("*|*:first-of-type", "").is_err());
        assert!(t.css_to_xpath("|*:first-of-type", "").is_err());
        // :lang()/:dir() argument validation; a lone '-' is not a valid
        // ident.
        assert!(t.css_to_xpath(":lang()", "").is_err());
        assert!(t.css_to_xpath(":lang(5)", "").is_err());
        assert!(t.css_to_xpath(":lang(-)", "").is_err());
        // A namespace prefix that is not a valid XPath name cannot be a
        // node test, and XPath 1.0 cannot resolve it without the
        // namespace URI: comparing the whole `prefix:name` against
        // `name()` would match only documents using that very prefix.
        let unsafe_prefix =
            crate::Error::Unsupported("a namespace prefix that needs quoting (`1ns`)".to_owned());
        assert_eq!(
            t.css_to_xpath("\\31 ns|div", "").unwrap_err(),
            unsafe_prefix
        );
        assert_eq!(t.css_to_xpath("\\31 ns|*", "").unwrap_err(), unsafe_prefix);
        assert_eq!(
            t.css_to_xpath("\\31 ns|di\\[v", "").unwrap_err(),
            unsafe_prefix
        );
        assert_eq!(
            t.css_to_xpath("[\\31 ns|href]", "").unwrap_err(),
            unsafe_prefix
        );
        assert_eq!(
            t.css_to_xpath("[\\31 ns|href='v']", "").unwrap_err(),
            unsafe_prefix
        );
        assert_eq!(
            t.css_to_xpath("e:is(\\31 ns|div)", "").unwrap_err(),
            unsafe_prefix
        );
        assert_eq!(
            t.css_to_xpath("e:has(> \\31 ns|div)", "").unwrap_err(),
            unsafe_prefix
        );
        // An+B must be whitespace-exact and integer-valued.
        assert!(t.css_to_xpath("e:nth-child(3 7)", "").is_err());
        assert!(t.css_to_xpath("e:nth-child(2 n)", "").is_err());
        assert!(t.css_to_xpath("e:nth-child(2.5)", "").is_err());
        assert!(t.css_to_xpath("e:nth-child(2e1)", "").is_err());
    }

    /// CSS syntax errors on malformed selectors, ported from selectr's
    /// parse-error suite. selectr pins its own hand-written parser's
    /// exact message text; css-to-xpath parses through Servo's `selectors`
    /// crate, so only the fact of an error is asserted here (message
    /// wording is pinned separately in `error_messages`).
    #[test]
    fn parse_errors() {
        let t = Translator::new(Mode::Generic);
        // Dangling/missing selectors around commas and combinators.
        assert!(t.css_to_xpath("div, ", "").is_err());
        assert!(t.css_to_xpath(" , div", "").is_err());
        assert!(t.css_to_xpath("p, , div", "").is_err());
        assert!(t.css_to_xpath("div > ", "").is_err());
        assert!(t.css_to_xpath("  > div", "").is_err());
        assert!(t.css_to_xpath(" ", "").is_err());
        // Malformed namespace syntax.
        assert!(t.css_to_xpath("foo|#bar", "").is_err());
        assert!(t.css_to_xpath("e|", "").is_err());
        assert!(t.css_to_xpath("div .|x", "").is_err());
        // A selector cannot start with a bare '#' or ':' before a class
        // token, nor a bare '.' before a hash/pseudo token.
        assert!(t.css_to_xpath("#.foo", "").is_err());
        assert!(t.css_to_xpath(".#foo", "").is_err());
        assert!(t.css_to_xpath(":#foo", "").is_err());
        // Malformed attribute selectors.
        assert!(t.css_to_xpath("[*]", "").is_err());
        assert!(t.css_to_xpath("[foo|]", "").is_err());
        assert!(t.css_to_xpath("[#]", "").is_err());
        assert!(t.css_to_xpath("[foo=#]", "").is_err());
        assert!(t.css_to_xpath("[href]a", "").is_err());
        assert!(t.css_to_xpath("[rel:stylesheet]", "").is_err());
        // :nth-child() requires at least one argument.
        assert!(t.css_to_xpath(":nth-child()", "").is_err());
        // Stray/invalid characters.
        assert!(t.css_to_xpath("attributes(href)/html/body/a", "").is_err());
        assert!(t.css_to_xpath("attributes(href)", "").is_err());
        assert!(t.css_to_xpath("html/body/a", "").is_err());
        assert!(t.css_to_xpath("foo!", "").is_err());
        assert!(t.css_to_xpath("a[rel!=nofollow]", "").is_err());
        assert!(t.css_to_xpath("a:not(b;)", "").is_err());
        // Mis-placed pseudo-elements: not at the end of a selector, or
        // anywhere inside a functional pseudo-class's argument.
        assert!(t.css_to_xpath("a:before:empty", "").is_err());
        assert!(t.css_to_xpath("li:before a", "").is_err());
        assert!(t.css_to_xpath(":not(:before)", "").is_err());
        assert!(t.css_to_xpath(":not(a,)", "").is_err());
        assert!(t.css_to_xpath(":is(:before)", "").is_err());
        assert!(t.css_to_xpath(":matches(:before)", "").is_err());
        assert!(t.css_to_xpath(":is(a:before b)", "").is_err());
        assert!(t.css_to_xpath(":is(a b:before)", "").is_err());
        // A trailing combinator inside a functional pseudo-class's
        // argument is still a dangling combinator.
        assert!(t.css_to_xpath(":is(a >)", "").is_err());
        // The corresponding well-formed selectors are valid.
        assert!(t.css_to_xpath("[rel=stylesheet]", "").is_ok());
        assert!(t.css_to_xpath(":lang(fr)", "").is_ok());
    }

    /// The nth-* family and its an+b arithmetic.
    #[test]
    fn nth_family() {
        assert_eq!(
            xpath("e:nth-child(1)"),
            "e[count(preceding-sibling::*) = 0]"
        );
        assert_eq!(
            xpath("e:nth-child(3n+2)"),
            "e[count(preceding-sibling::*) >= 1 and (count(preceding-sibling::*) +2) mod 3 = 0]"
        );
        assert_eq!(
            xpath("e:nth-child(3n-2)"),
            "e[count(preceding-sibling::*) mod 3 = 0]"
        );
        assert_eq!(
            xpath("e:nth-child(-n+6)"),
            "e[count(preceding-sibling::*) <= 5]"
        );
        assert_eq!(xpath("e:nth-child(n)"), "e");
        assert_eq!(xpath("e:nth-child(odd)"), xpath("e:nth-child(2n+1)"));
        assert_eq!(xpath("e:nth-child(even)"), xpath("e:nth-child(2n)"));
        // An+B is ASCII case-insensitive per css-syntax; Servo handles it
        // natively.
        assert_eq!(xpath("e:nth-child(2N)"), xpath("e:nth-child(2n)"));
        assert_eq!(xpath("e:nth-child(ODD)"), xpath("e:nth-child(odd)"));
        assert_eq!(xpath("e:nth-child(EVEN)"), xpath("e:nth-child(even)"));
        assert_eq!(xpath("e:nth-child(-N+3)"), xpath("e:nth-child(-n+3)"));
        assert_eq!(
            xpath("e:nth-last-child(1)"),
            "e[count(following-sibling::*) = 0]"
        );
        assert_eq!(
            xpath("e:nth-last-child(2n)"),
            "e[(count(following-sibling::*) +1) mod 2 = 0]"
        );
        assert_eq!(
            xpath("e:nth-last-child(2n+1)"),
            "e[count(following-sibling::*) mod 2 = 0]"
        );
        assert_eq!(
            xpath("e:nth-last-child(2n+2)"),
            "e[count(following-sibling::*) >= 1 and (count(following-sibling::*) +1) mod 2 = 0]"
        );
        assert_eq!(
            xpath("e:nth-last-child(3n+1)"),
            "e[count(following-sibling::*) mod 3 = 0]"
        );
        assert_eq!(
            xpath("e:nth-last-child(-n+2)"),
            "e[count(following-sibling::*) <= 1]"
        );
        assert_eq!(
            xpath("e:nth-of-type(1)"),
            "e[count(preceding-sibling::e) = 0]"
        );
        assert_eq!(
            xpath("e:nth-last-of-type(1)"),
            "e[count(following-sibling::e) = 0]"
        );
        assert_eq!(
            xpath("div e:nth-last-of-type(1) .aclass"),
            "div//e[count(following-sibling::e) = 0]//*[@class and contains(concat(' ', normalize-space(@class), ' '), ' aclass ')]"
        );
        // Servo collapses :first-child & co. into nth data; the general
        // an+b form covers them (see translate::nth).
        assert_eq!(xpath("e:first-child"), "e[count(preceding-sibling::*) = 0]");
        assert_eq!(xpath("e:last-child"), "e[count(following-sibling::*) = 0]");
        assert_eq!(
            xpath("e:first-of-type"),
            "e[count(preceding-sibling::e) = 0]"
        );
        assert_eq!(
            xpath("e:last-of-type"),
            "e[count(following-sibling::e) = 0]"
        );
        assert_eq!(
            xpath("e:only-child"),
            "e[count(preceding-sibling::*) = 0 and count(following-sibling::*) = 0]"
        );
        assert_eq!(
            xpath("e:only-of-type"),
            "e[count(preceding-sibling::e) = 0 and count(following-sibling::e) = 0]"
        );
        // Element names needing quoting fold into a namespace-pinned
        // name() condition; the of-type pseudos count same-type siblings
        // through the same test.
        assert_eq!(
            xpath("é:first-of-type"),
            "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]"
        );
        assert_eq!(
            xpath("é:nth-of-type(2)"),
            "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 1]"
        );
        assert_eq!(
            xpath("é:nth-last-of-type(1)"),
            "*[name() = 'é' and namespace-uri() = '' and count(following-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]"
        );
        assert_eq!(
            xpath("é:only-of-type"),
            "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 0 and count(following-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]"
        );
        // Explicit-namespace and no-namespace elements go through
        // local-name()/name()-plus-namespace-uri() the same way.
        assert_eq!(
            xpath("*|e:first-of-type"),
            "*[local-name() = 'e' and count(preceding-sibling::*[local-name() = 'e']) = 0]"
        );
        assert_eq!(
            xpath("|é:first-of-type"),
            "*[name() = 'é' and namespace-uri() = '' and count(preceding-sibling::*[name() = 'é' and namespace-uri() = '']) = 0]"
        );
        assert_eq!(
            xpath("e ~ f:nth-child(3)"),
            "e/following-sibling::f[count(preceding-sibling::*) = 2]"
        );
        // Early exits: a=1 with b<=1 matches everything; a<0 with b<1 is
        // impossible.
        assert_eq!(xpath("e:nth-child(n+1)"), "e");
        assert_eq!(xpath("e:nth-child(n-5)"), "e");
        assert_eq!(xpath("e:nth-child(-n)"), "e[0]");
        assert_eq!(xpath("e:nth-child(-2n-1)"), "e[0]");
        assert_eq!(xpath("e:nth-child(-n+0)"), "e[0]");
        assert_eq!(
            xpath("e:nth-child(-n+1)"),
            "e[count(preceding-sibling::*) <= 0]"
        );
        assert_eq!(
            xpath("e:nth-child(-2n+2)"),
            "e[count(preceding-sibling::*) <= 1 and (count(preceding-sibling::*) +1) mod -2 = 0]"
        );
    }

    /// `of S` selector lists (CSS Level 4), nth-child only.
    #[test]
    fn nth_child_of() {
        assert_eq!(
            xpath("div:nth-child(2 of .foo)"),
            "div[count(preceding-sibling::*[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]) = 1 and @class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]"
        );
        // a=1, b<=1: only the current-element check remains.
        assert_eq!(
            xpath("li:nth-child(n of .item)"),
            "li[@class and contains(concat(' ', normalize-space(@class), ' '), ' item ')]"
        );
        // Impossible series keeps the current-element check after the 0.
        assert_eq!(
            xpath("li:nth-child(-n of .item)"),
            "li[0 and @class and contains(concat(' ', normalize-space(@class), ' '), ' item ')]"
        );
        // An element argument folds into a self:: test.
        assert_eq!(
            xpath("div:nth-child(2 of div.foo)"),
            "div[count(preceding-sibling::*[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::div]) = 1 and @class and contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::div]"
        );
        // A universal argument makes the list match everything, like a
        // plain :nth-child.
        assert_eq!(
            xpath("li:nth-child(2 of .foo, *)"),
            "li[count(preceding-sibling::*) = 1]"
        );
    }

    /// The size of an `of S` translation is bounded. XPath 1.0 has no
    /// variables, so `S` is written out twice — into the sibling
    /// predicate and into the current-element check — and a nested `of S`
    /// lands in both copies, doubling the output per level. Only a limit
    /// can fix that, so both the nesting depth and the size of one level
    /// are capped.
    #[test]
    fn nth_child_of_nesting_is_bounded() {
        let t = Translator::new(Mode::Generic);
        let nest = |n: usize| format!("{}a{}", ":nth-child(2 of ".repeat(n), ")".repeat(n));
        let too_deep = crate::Error::Unsupported(
            "`An+B of S` selector lists nested more than 8 levels deep".to_owned(),
        );

        // Two levels, in full: `a` appears four times, not twice.
        assert_eq!(
            xpath(&nest(2)),
            "*[count(preceding-sibling::*[count(preceding-sibling::*[self::a]) = 1 \
              and self::a]) = 1 \
              and count(preceding-sibling::*[self::a]) = 1 and self::a]"
        );
        // The doubling itself: each level is a little over twice the last.
        assert_eq!(xpath(&nest(4)).len(), 685);
        assert_eq!(xpath(&nest(8)).len(), 11_485);

        // Past the limit it is an error, and a cheap one: the depth is
        // checked before descending, so nothing exponential is built.
        assert_eq!(t.css_to_xpath(&nest(9), "").unwrap_err(), too_deep);
        assert_eq!(t.css_to_xpath(&nest(40), "").unwrap_err(), too_deep);
        // The selector is 154 bytes, so its quote is elided at 120
        // with a `…`: every message is bounded, not just the
        // caret-bearing ones.
        assert_eq!(
            too_deep.into_message(&nest(9)),
            format!(
                "The CSS selector \"{}\u{2026}\" uses `An+B of S` selector lists nested \
                 more than 8 levels deep, which this translator does not support",
                &nest(9)[..120]
            )
        );

        // Depth counts `of S` lists wherever they sit, including inside
        // another functional pseudo-class.
        let laundered =
            |n: usize| format!("{}a{}", ":nth-child(2 of :is(".repeat(n), "))".repeat(n));
        assert!(t.css_to_xpath(&laundered(8), "").is_ok());
        assert_eq!(
            t.css_to_xpath(&laundered(9), "").unwrap_err(),
            crate::Error::Unsupported(
                "`An+B of S` selector lists nested more than 8 levels deep".to_owned()
            )
        );

        // The depth limit bounds the doubling, not what is doubled, so a
        // large argument at full depth is capped by size as well.
        let big = |k: usize| {
            let args: Vec<String> = (0..k).map(|i| format!("a{i}")).collect();
            format!(
                "{}:is({}){}",
                ":nth-child(2 of ".repeat(8),
                args.join(","),
                ")".repeat(8)
            )
        };
        assert!(t.css_to_xpath(&big(200), "").is_ok());
        assert_eq!(
            t.css_to_xpath(&big(800), "").unwrap_err(),
            crate::Error::Unsupported(
                "an `An+B of S` selector list translating to more than 1048576 bytes".to_owned()
            )
        );
    }

    /// Structural pseudos and the generic never-match set.
    #[test]
    fn structural_and_never_match_pseudos() {
        assert_eq!(xpath("e:empty"), "e[not(*) and not(string-length())]");
        assert_eq!(xpath("e:EmPTY"), "e[not(*) and not(string-length())]");
        assert_eq!(xpath("e:root"), "e[not(parent::*)]");
        // The generic never-match set.
        for pseudo in [
            "any-link",
            "link",
            "visited",
            "hover",
            "active",
            "focus",
            "focus-within",
            "focus-visible",
            "target",
            "target-within",
            "local-link",
            "enabled",
            "disabled",
            "checked",
            "required",
            "optional",
        ] {
            assert_eq!(xpath(&format!("a:{pseudo}")), "a[0]");
        }
        assert_eq!(xpath("a:dir(ltr)"), "a[0]");
    }

    #[test]
    fn negation_matching_where_has() {
        assert_eq!(
            xpath("e:not(:nth-child(odd))"),
            "e[not(count(preceding-sibling::*) mod 2 = 0)]"
        );
        assert_eq!(xpath("e:nOT(*)"), "e[0]");
        assert_eq!(xpath("e:not(a)"), "e[not(self::a)]");
        assert_eq!(xpath(":not(*|e)"), "*[not(local-name() = 'e')]");
        assert_eq!(xpath("e:not(a, b)"), "e[not(self::a or self::b)]");
        // A universal argument makes :not() unmatchable...
        assert_eq!(xpath("div:not(a, *)"), "div[0]");
        // :where() / :is() OR their arguments together into one condition
        // that ANDs with the rest of the compound.
        assert_eq!(xpath("div:where(p)"), "div[self::p]");
        assert_eq!(xpath("div:where(p, span)"), "div[self::p or self::span]");
        assert_eq!(xpath("section:where(#main)"), "section[@id = 'main']");
        assert_eq!(xpath("input:where([required])"), "input[@required]");
        assert_eq!(
            xpath("*:where(.highlight)"),
            "*[@class and contains(concat(' ', normalize-space(@class), ' '), ' highlight ')]"
        );
        assert_eq!(
            xpath("div:where(.foo, .bar)"),
            "div[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ') or @class and contains(concat(' ', normalize-space(@class), ' '), ' bar ')]"
        );
        assert_eq!(
            xpath("p:where(.highlight, #special, [data-key])"),
            "p[@class and contains(concat(' ', normalize-space(@class), ' '), ' highlight ') or @id = 'special' or @data-key]"
        );
        assert_eq!(
            xpath("*:where(div.content)"),
            "*[@class and contains(concat(' ', normalize-space(@class), ' '), ' content ') and self::div]"
        );
        assert_eq!(
            xpath("div:where(p):where(span)"),
            "div[self::p and self::span]"
        );
        assert_eq!(xpath("div:is(p)"), "div[self::p]");
        // :matches() is the legacy alias for :is().
        assert_eq!(xpath("div:matches(p)"), "div[self::p]");
        // ...and :is()/:where() a no-op constraint.
        assert_eq!(xpath("e:is(*)"), "e");
        assert_eq!(xpath("div:is(a, *)"), "div");
        assert_eq!(xpath("div:where(a, *)"), "div");
        // :has().
        assert_eq!(xpath("div:has(p)"), "div[.//p]");
        assert_eq!(
            xpath("div:has(.foo)"),
            "div[.//*[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]"
        );
        assert_eq!(xpath("div:has(p, span)"), "div[.//p | .//span]");
        assert_eq!(xpath("div:has(p):has(span)"), "div[.//p and .//span]");
        assert_eq!(
            xpath("section:has(div.content)"),
            "section[.//div[@class and contains(concat(' ', normalize-space(@class), ' '), ' content ')]]"
        );
        assert_eq!(xpath("div:has(*)"), "div[.//*]");
        assert_eq!(xpath("section:has(#main)"), "section[.//*[@id = 'main']]");
        assert_eq!(xpath("form:has([required])"), "form[.//*[@required]]");
        assert_eq!(xpath("*:has(img)"), "*[.//img]");
        // Leading combinators in :has() (selectors-4 relative selectors).
        assert_eq!(xpath("e:has(> img)"), "e[child::img]");
        assert_eq!(xpath("e:has(~ p)"), "e[following-sibling::p]");
        assert_eq!(xpath("e:has(+ p)"), "e[following-sibling::*[1][self::p]]");
        assert_eq!(
            xpath("e:has(> a, ~ p)"),
            "e[child::a | following-sibling::p]"
        );
        assert_eq!(
            xpath("e:has(> .foo)"),
            "e[child::*[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]"
        );
        assert_eq!(
            xpath("e:has(+ p.foo)"),
            "e[following-sibling::*[1][@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ') and self::p]]"
        );
        // Nested :not() (Selectors Level 4).
        assert_eq!(xpath(":not(:not(a))"), "*[not(not(self::a))]");
        assert_eq!(xpath("e:is(:not(f))"), "e[not(self::f)]");
        assert_eq!(xpath("e:has(:not(f))"), "e[.//*[not(self::f)]]");
        // Prefixed names inside arguments stay node tests, resolved
        // through the namespace map like a top-level `svg|g` — not a
        // string comparison against the document's prefix.
        assert_eq!(xpath("e:is(svg|g)"), "e[self::svg:g]");
        assert_eq!(xpath("e:not(svg|g)"), "e[not(self::svg:g)]");
        assert_eq!(xpath("e:is(svg|*)"), "e[self::svg:*]");
        assert_eq!(xpath("e:has(svg|g)"), "e[.//svg:g]");
        assert_eq!(xpath("e:has(> svg|g)"), "e[child::svg:g]");
        assert_eq!(xpath("e:has(~ svg|g)"), "e[following-sibling::svg:g]");
        assert_eq!(
            xpath("e:has(+ svg|g)"),
            "e[following-sibling::*[1][self::svg:g]]"
        );
        assert_eq!(
            xpath("e:has(svg|g.foo)"),
            "e[.//svg:g[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]]"
        );
    }

    /// An empty `:is()` / `:where()` argument list. Selectors 4 makes
    /// those lists forgiving, so an empty one is valid and matches
    /// nothing — but nothing else about forgiveness is adopted: an
    /// argument that fails to parse is still an error rather than a
    /// silently dropped one.
    #[test]
    fn empty_forgiving_argument_lists() {
        let t = Translator::new(Mode::Generic);
        assert_eq!(xpath(":is()"), "*[0]");
        assert_eq!(xpath(":where()"), "*[0]");
        assert_eq!(xpath("a:where()"), "a[0]");
        assert_eq!(xpath("e:matches()"), "e[0]");
        // The name is matched case-insensitively, and the argument list
        // is empty when it holds no tokens, not only when it is empty.
        assert_eq!(xpath(":IS( )"), "*[0]");
        assert_eq!(xpath(":is(/**/)"), "*[0]");
        // Nested inside the non-forgiving pseudo-classes, whose own
        // empty argument lists stay errors.
        assert_eq!(xpath("a:not(:is())"), "a[not(0)]");
        assert_eq!(xpath("div:has(:is())"), "div[.//*[0]]");
        assert!(t.css_to_xpath(":not()", "").is_err());
        assert!(t.css_to_xpath(":has()", "").is_err());
        assert!(t.css_to_xpath(":nth-child(2 of)", "").is_err());
        // An empty *argument* is a dropped argument, not an empty list.
        assert!(t.css_to_xpath(":is(a,)", "").is_err());
        assert!(t.css_to_xpath(":is(,a)", "").is_err());
        assert!(t.css_to_xpath(":is( , )", "").is_err());
        assert!(t.css_to_xpath(":where(a,)", "").is_err());
        // An argument that fails to parse is reported, not dropped, and
        // the error names it where it stands.
        assert_eq!(
            t.css_to_xpath(":is(a, ::before)", "").unwrap_err(),
            crate::Error::Parse("InvalidState".to_owned(), 15)
        );
        // An empty list no longer being an error, the error reported for
        // a selector holding one is the next thing that is wrong.
        assert_eq!(
            t.css_to_xpath(":is() > ::after", "").unwrap_err(),
            crate::Error::Parse("UnsupportedPseudoClassOrElement(\"after\")".to_owned(), 9)
        );
    }

    /// Complex selectors (with combinators) inside the functional
    /// pseudo-classes (Selectors Level 4). :is()/:where()/:not() and the
    /// nth `of S` lists match their argument at the candidate element, so
    /// each combinator becomes an existence test through the reversed
    /// axis; :has() looks forward, extending its path compound by
    /// compound.
    #[test]
    fn complex_pseudo_arguments() {
        // One reversed axis per combinator.
        assert_eq!(xpath("e:is(a b)"), "e[self::b and ancestor::*[self::a]]");
        assert_eq!(xpath("e:is(a > b)"), "e[self::b and parent::*[self::a]]");
        assert_eq!(
            xpath("e:is(a + b)"),
            "e[self::b and preceding-sibling::*[1][self::a]]"
        );
        assert_eq!(
            xpath("e:is(a ~ b)"),
            "e[self::b and preceding-sibling::*[self::a]]"
        );
        // Longer chains nest, each step wrapping the remainder.
        assert_eq!(
            xpath("e:is(a b c)"),
            "e[self::c and ancestor::*[self::b and ancestor::*[self::a]]]"
        );
        assert_eq!(
            xpath("e:is(a > b ~ c)"),
            "e[self::c and preceding-sibling::*[self::b and parent::*[self::a]]]"
        );
        assert_eq!(
            xpath("e:is(a + b > c)"),
            "e[self::c and parent::*[self::b and preceding-sibling::*[1][self::a]]]"
        );
        // :not() negates the whole chain condition; complex and compound
        // arguments OR together ('and' binds tighter than 'or').
        assert_eq!(
            xpath("e:not(a b)"),
            "e[not(self::b and ancestor::*[self::a])]"
        );
        assert_eq!(
            xpath("e:not(a > b + c)"),
            "e[not(self::c and preceding-sibling::*[1][self::b and parent::*[self::a]])]"
        );
        assert_eq!(
            xpath("e:is(a b, c)"),
            "e[self::b and ancestor::*[self::a] or self::c]"
        );
        assert_eq!(
            xpath("e:is(a, b c)"),
            "e[self::a or self::c and ancestor::*[self::b]]"
        );
        // Universal steps: a bare-`*` left-hand side is a bare axis test,
        // a bare-`*` rightmost compound leaves only the chain test, and a
        // universal *argument* still makes the list trivially true (or
        // :not() unmatchable).
        assert_eq!(xpath("e:is(* b)"), "e[self::b and ancestor::*]");
        assert_eq!(xpath("e:is(a *)"), "e[ancestor::*[self::a]]");
        assert_eq!(xpath("e:not(a *)"), "e[not(ancestor::*[self::a])]");
        assert_eq!(xpath("e:is(a b, *)"), "e");
        assert_eq!(xpath("e:not(a b, *)"), "e[0]");
        // Conditions on chain steps come before each step's name test.
        assert_eq!(
            xpath("e:is(a.x b.y)"),
            "e[@class and contains(concat(' ', normalize-space(@class), ' '), ' y ') and \
             self::b and \
             ancestor::*[@class and contains(concat(' ', normalize-space(@class), ' '), ' x ') \
             and self::a]]"
        );
        assert_eq!(
            xpath("e:is(a[foo='bar'] > b)"),
            "e[self::b and parent::*[@foo = 'bar' and self::a]]"
        );
        assert_eq!(
            xpath("e:is(a:first-child b)"),
            "e[self::b and ancestor::*[count(preceding-sibling::*) = 0 and self::a]]"
        );
        assert_eq!(
            xpath("e:is(a:hover b)"),
            "e[self::b and ancestor::*[0 and self::a]]"
        );
        // Nested pseudo-classes inside chain steps; an or-group condition
        // is parenthesized when conjoined with the chain test.
        assert_eq!(
            xpath("e:is(:not(a) b)"),
            "e[self::b and ancestor::*[not(self::a)]]"
        );
        assert_eq!(
            xpath("e:not(:is(a b))"),
            "e[not(self::b and ancestor::*[self::a])]"
        );
        assert_eq!(
            xpath("e:is(:not(a b) c)"),
            "e[self::c and ancestor::*[not(self::b and ancestor::*[self::a])]]"
        );
        assert_eq!(
            xpath("e:is(:is(a, b) c)"),
            "e[self::c and ancestor::*[self::a or self::b]]"
        );
        assert_eq!(
            xpath("e:is(c :is(a, b))"),
            "e[(self::a or self::b) and ancestor::*[self::c]]"
        );
        // Prefixed names in chain steps stay self:: node tests.
        assert_eq!(
            xpath("ns|e:is(a b)"),
            "ns:e[self::b and ancestor::*[self::a]]"
        );
        assert_eq!(
            xpath("e:is(ns|a b)"),
            "e[self::b and ancestor::*[self::ns:a]]"
        );
        assert_eq!(
            xpath("e:is(a ns|b)"),
            "e[self::ns:b and ancestor::*[self::a]]"
        );
        // :has() walks forward: one joiner per combinator, with the
        // leading combinator choosing the first axis.
        assert_eq!(xpath("e:has(a b)"), "e[.//a//b]");
        assert_eq!(xpath("e:has(a > b)"), "e[.//a/b]");
        assert_eq!(
            xpath("e:has(a + b)"),
            "e[.//a/following-sibling::*[1][self::b]]"
        );
        assert_eq!(xpath("e:has(a ~ b)"), "e[.//a/following-sibling::b]");
        assert_eq!(xpath("e:has(> a b)"), "e[child::a//b]");
        assert_eq!(xpath("e:has(> a > b)"), "e[child::a/b]");
        assert_eq!(
            xpath("e:has(+ a > b)"),
            "e[following-sibling::*[1][self::a]/b]"
        );
        assert_eq!(
            xpath("e:has(~ a + b)"),
            "e[following-sibling::a/following-sibling::*[1][self::b]]"
        );
        assert_eq!(xpath("e:has(~ a > b)"), "e[following-sibling::a/b]");
        assert_eq!(
            xpath("e:has(a > b + c)"),
            "e[.//a/b/following-sibling::*[1][self::c]]"
        );
        assert_eq!(
            xpath("e:has(> a:is(b c))"),
            "e[child::a[self::c and ancestor::*[self::b]]]"
        );
        assert_eq!(
            xpath("e:has(a.x > b.y)"),
            "e[.//a[@class and contains(concat(' ', normalize-space(@class), ' '), ' x ')]\
             /b[@class and \
             contains(concat(' ', normalize-space(@class), ' '), ' y ')]]"
        );
        // Prefixed names stay path node tests, except under `+` where the
        // [1] position predicate needs the node test to stay `*`.
        assert_eq!(xpath("e:has(ns|a > b)"), "e[.//ns:a/b]");
        assert_eq!(
            xpath("e:has(a + ns|b)"),
            "e[.//a/following-sibling::*[1][self::ns:b]]"
        );
        // `of S` with complex selectors: the chain condition filters the
        // counted siblings and constrains the current element.
        assert_eq!(
            xpath("e:nth-child(2n of a b)"),
            "e[(count(preceding-sibling::*[self::b and ancestor::*[self::a]]) +1) \
             mod 2 = 0 and self::b and ancestor::*[self::a]]"
        );
        assert_eq!(
            xpath("e:nth-child(2n of a > b)"),
            "e[(count(preceding-sibling::*[self::b and parent::*[self::a]]) +1) \
             mod 2 = 0 and self::b and parent::*[self::a]]"
        );
        assert_eq!(
            xpath("e:nth-last-child(3 of a b)"),
            "e[count(following-sibling::*[self::b and ancestor::*[self::a]]) = 2 \
             and self::b and ancestor::*[self::a]]"
        );
    }

    /// :scope (Selectors Level 4) anchors the expression at the node the
    /// XPath is evaluated from: the leftmost compound moves onto the
    /// self:: axis and the prefix is not applied.
    #[test]
    fn scope_pseudo() {
        let t = Translator::new(Mode::Generic);
        assert_eq!(xpath(":scope"), "self::*");
        assert_eq!(xpath(":ScoPE"), "self::*");
        assert_eq!(xpath(":scope > a"), "self::*/a");
        assert_eq!(xpath(":scope a"), "self::*//a");
        assert_eq!(
            xpath(":scope + a"),
            "self::*/following-sibling::*[1][self::a]"
        );
        assert_eq!(xpath(":scope ~ a"), "self::*/following-sibling::a");
        // Other simple selectors in the :scope compound constrain the
        // context node itself.
        assert_eq!(xpath("div:scope"), "self::div");
        assert_eq!(xpath("svg|g:scope"), "self::svg:g");
        assert_eq!(
            xpath(":scope.foo > a"),
            "self::*[@class and contains(concat(' ', normalize-space(@class), ' '), ' foo ')]/a"
        );
        assert_eq!(
            xpath(":scope:first-child"),
            "self::*[count(preceding-sibling::*) = 0]"
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

    #[test]
    fn lang_and_dir() {
        /// Strip the outer `E[`…`]` predicate off a single-condition
        /// translation, to rebuild the expected OR of a comma list.
        fn inner(xpath: &str) -> &str {
            xpath
                .strip_prefix("E[")
                .and_then(|s| s.strip_suffix(']'))
                .unwrap()
        }

        // Generic: XPath's lang() does prefix matching natively.
        assert_eq!(xpath("e:lang(en)"), "e[lang('en')]");
        assert_eq!(xpath("e:lang(\"en\")"), "e[lang('en')]");
        assert_eq!(xpath("e:lang(en-*)"), "e[lang('en')]");
        // A bare * matches a *known* language, which XPath's lang() cannot
        // express: walk xml:lang instead (xml:lang="" is unknown).
        assert_eq!(
            xpath("e:lang(*)"),
            "e[ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0]]"
        );
        assert_eq!(xpath("e:lang(en, fr)"), "e[lang('en') or lang('fr')]");
        assert_eq!(
            xpath("e:lang(en, de, fr)"),
            "e[lang('en') or lang('de') or lang('fr')]"
        );
        // Whitespace around the commas is fine.
        assert_eq!(xpath("e:lang( en , fr )"), "e[lang('en') or lang('fr')]");
        // But whitespace alone is not a separator (selectors-4 wants a
        // comma-separated list), and it does not glue a range together
        // either. `en*` is the case that matters most: read as the two
        // ranges `en` and `*`, a typo would quietly widen the selector
        // to every element with a known language.
        for sel in [
            "e:lang(en fr)",
            "e:lang(en *)",
            "e:lang(en*)",
            // Empty and empty-subtag ranges cannot match anything and
            // are not valid language ranges.
            "e:lang(\"\")",
            "e:lang(en-)",
            "e:lang(en--)",
            "e:lang(--x)",
            "e:lang(en,)",
            "e:lang(en,,fr)",
        ] {
            assert!(
                Translator::new(Mode::Generic)
                    .css_to_xpath(sel, "")
                    .is_err(),
                "{sel} should error"
            );
        }
        // A bare * stays match-anything even alongside other ranges: it
        // must not be confused with the head of an interior wildcard.
        assert_eq!(
            xpath("e:lang(*, fr)"),
            "e[ancestor-or-self::*[@xml:lang][1][string-length(@xml:lang) > 0] or lang('fr')]"
        );
        // HTML: nearest lang-attributed ancestor, lowercased prefix match.
        let html = Translator::new(Mode::Html);
        assert_eq!(
            html.css_to_xpath("e:lang(EN)", "").unwrap(),
            "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')]]"
        );
        assert_eq!(
            html.css_to_xpath("e:lang(*)", "").unwrap(),
            "e[ancestor-or-self::*[@lang][1][string-length(@lang) > 0]]"
        );
        // A trailing wildcard matches the same prefix as the range
        // without it: both stop at a subtag boundary.
        assert_eq!(
            html.css_to_xpath("e:lang(en-*)", "").unwrap(),
            html.css_to_xpath("e:lang(en)", "").unwrap()
        );
        // The range is ASCII-lowercased, matching the XPath
        // translate() alphabet on the other side of the comparison.
        assert_eq!(
            html.css_to_xpath("e:lang(T\u{dc}RK)", "").unwrap(),
            "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 't\u{dc}rk-')]]"
        );
        // A hyphenated range keeps its full spelling in the prefix match.
        assert_eq!(
            html.css_to_xpath("e:lang(en-nz)", "").unwrap(),
            "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-nz-')]]"
        );
        // A comma list ORs the per-range ancestor-or-self:: tests.
        assert_eq!(
            html.css_to_xpath("e:lang(en, fr)", "").unwrap(),
            "e[ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'en-')] or ancestor-or-self::*[@lang][1][starts-with(concat(translate(@lang, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '-'), 'fr-')]]"
        );
        // xhtml shares the HTML overrides but reads either language
        // attribute: XHTML documents conventionally carry xml:lang, often
        // alongside lang, and HTML's language determination prefers
        // xml:lang when both sit on the same element. XPath 1.0 has no
        // conditional, so the lang half is truncated to zero length
        // whenever xml:lang is present.
        let xhtml = Translator::new(Mode::Xhtml);
        assert_eq!(
            xhtml.css_to_xpath("E:lang(*)", "").unwrap(),
            "E[ancestor-or-self::*[@xml:lang or @lang][1]\
             [string-length(concat(@xml:lang, \
             substring(@lang, 1, string-length(@lang) * not(@xml:lang)))) > 0]]"
        );
        assert_eq!(
            xhtml.css_to_xpath("E:lang(EN)", "").unwrap(),
            "E[ancestor-or-self::*[@xml:lang or @lang][1]\
             [starts-with(concat(translate(concat(@xml:lang, \
             substring(@lang, 1, string-length(@lang) * not(@xml:lang))), \
             'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), \
             '-'), 'en-')]]"
        );
        // The rest of the range handling is shared with Mode::Html: a
        // trailing wildcard matches the same prefix as the bare range, a
        // hyphenated range keeps its full spelling, and a comma list ORs.
        assert_eq!(
            xhtml.css_to_xpath("E:lang(en-*)", "").unwrap(),
            xhtml.css_to_xpath("E:lang(en)", "").unwrap()
        );
        assert!(
            xhtml
                .css_to_xpath("E:lang(en-nz)", "")
                .unwrap()
                .ends_with("'-'), 'en-nz-')]]")
        );
        assert_eq!(
            xhtml.css_to_xpath("E:lang(en, fr)", "").unwrap(),
            format!(
                "E[{} or {}]",
                inner(&xhtml.css_to_xpath("E:lang(en)", "").unwrap()),
                inner(&xhtml.css_to_xpath("E:lang(fr)", "").unwrap()),
            )
        );
        // Mode::Html stays lang-only: an HTML parser puts a literal
        // `xml:lang` in no namespace, which HTML ignores when determining
        // an element's language.
        assert!(
            !html
                .css_to_xpath("e:lang(en)", "")
                .unwrap()
                .contains("xml:lang")
        );
        assert!(
            !html
                .css_to_xpath("e:lang(*)", "")
                .unwrap()
                .contains("xml:lang")
        );
        // Interior wildcards (RFC 4647 extended filtering) are valid CSS
        // but inexpressible in XPath 1.0, so both spellings error rather
        // than over-match (unquoted *-CH) or never match (quoted "*-CH").
        let t = Translator::new(Mode::Generic);
        for sel in [
            "e:lang(*-CH)",
            "e:lang(\"*-CH\")",
            "e:lang(de-*-DE)",
            "e:lang(\"de-*-DE\")",
        ] {
            assert!(t.css_to_xpath(sel, "").is_err(), "{sel} should error");
            assert!(
                html.css_to_xpath(sel, "").is_err(),
                "{sel} should error (html)"
            );
        }
        // :dir() takes exactly one identifier (selectors-4) — none of
        // :lang()'s strings, wildcards, or lists. It never matches in any
        // translator: resolved directionality needs runtime bidi
        // resolution, and a nearest-@dir approximation is deliberately
        // not attempted (see apply_pseudo_class).
        assert_eq!(xpath("e:dir(rtl)"), "e[0]");
        assert_eq!(html.css_to_xpath("e:dir(rtl)", "").unwrap(), "e[0]");
        assert_eq!(xhtml.css_to_xpath("e:dir(ltr)", "").unwrap(), "e[0]");
        // Never-match applies regardless of the (valid) ident's value.
        assert_eq!(xpath("e:dir(foo)"), "e[0]");
        assert!(t.css_to_xpath("e:dir()", "").is_err());
        assert!(t.css_to_xpath("e:dir(ltr rtl)", "").is_err());
        assert!(t.css_to_xpath("e:dir(ltr, rtl)", "").is_err());
        assert!(t.css_to_xpath("e:dir(\"ltr\")", "").is_err());
        assert!(t.css_to_xpath("e:dir(*)", "").is_err());
    }

    /// The HTML translator's pseudo-class overrides.
    #[test]
    fn html_pseudo_overrides() {
        let html = Translator::new(Mode::Html);
        let h = |css: &str| html.css_to_xpath(css, "").unwrap();
        // :link is `a`/`area` with an @href; the `link` element has an
        // @href but is not one of the elements HTML matches here.
        assert_eq!(
            h("a:link"),
            "a[@href and (name(.) = 'a' or name(.) = 'area')]"
        );
        // :any-link is :link plus :visited; with no visited state in a
        // static document the two coincide, so they share a translation.
        assert_eq!(h("a:any-link"), h("a:link"));
        assert_eq!(h("a:ANY-link"), h("a:link"));
        // @type comparisons fold case (HTML enumerated attribute), so
        // type="RADIO" reads as a radio. The fold is the same translate()
        // the `i` attribute flag uses.
        let t_lc = "translate(@type, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz')";
        assert_eq!(
            h("input:checked"),
            format!(
                "input[(@selected and name(.) = 'option') or \
                 (@checked and name(.) = 'input' and \
                 ({t_lc} = 'checkbox' or {t_lc} = 'radio'))]"
            )
        );
        // :required/:optional test the @required attribute over the
        // elements it applies to; input types where it has no effect
        // match neither.
        assert_eq!(
            h("input:required"),
            format!(
                "input[@required and ((name(.) = 'input' and not(\
                 {t_lc} = 'hidden' or {t_lc} = 'range' or {t_lc} = 'color' or \
                 {t_lc} = 'submit' or {t_lc} = 'image' or {t_lc} = 'reset' or \
                 {t_lc} = 'button')) or name(.) = 'select' or name(.) = 'textarea')]"
            )
        );
        assert_eq!(
            h("select:optional"),
            format!(
                "select[not(@required) and ((name(.) = 'input' and not(\
                 {t_lc} = 'hidden' or {t_lc} = 'range' or {t_lc} = 'color' or \
                 {t_lc} = 'submit' or {t_lc} = 'image' or {t_lc} = 'reset' or \
                 {t_lc} = 'button')) or name(.) = 'select' or name(.) = 'textarea')]"
            )
        );
        // :disabled and :enabled test the same element set — HTML's
        // button/input/select/textarea/optgroup/option/fieldset, with no
        // hyperlinks and none of the obsolete keygen/command — against
        // the same "actually disabled" condition, negated for :enabled,
        // so the two always partition that set. The condition covers
        // @disabled, an option under a disabled optgroup (the parent,
        // per the spec, not any ancestor), and the fieldset carve-out:
        // a control or nested fieldset inside a disabled fieldset is
        // disabled unless it sits in that fieldset's first legend,
        // expressed by counting disabled-fieldset ancestors against
        // protecting first-legends.
        let set = "(name(.) = 'button' or name(.) = 'input' or \
                    name(.) = 'select' or name(.) = 'textarea' or \
                    name(.) = 'optgroup' or name(.) = 'option' or \
                    name(.) = 'fieldset')";
        let fd = "count(ancestor::fieldset[@disabled]) > \
                  count(ancestor::legend[not(preceding-sibling::legend)]\
                  [parent::fieldset[@disabled]])";
        let disabled = format!(
            "@disabled or \
             (name(.) = 'option' and parent::optgroup[@disabled]) or \
             (not(name(.) = 'optgroup' or name(.) = 'option') and {fd})"
        );
        assert_eq!(
            h("input:disabled"),
            format!("input[{set} and ({disabled})]")
        );
        assert_eq!(
            h("input:enabled"),
            format!("input[{set} and not({disabled})]")
        );
        // Hyperlinks and the obsolete `keygen`/`command` are in neither
        // set: the predicate is the same one whatever it hangs off, and
        // it tests no name outside the set, so `a:enabled` (like
        // `keygen:enabled`) can never match.
        assert_eq!(h("a:enabled"), h("input:enabled").replace("input[", "a["));
        for css in ["a:enabled", "a:disabled", "input:checked"] {
            assert!(!h(css).contains("keygen"), "{css}");
            assert!(!h(css).contains("command"), "{css}");
        }
        assert!(!h("a:enabled").contains("@href"));
        // The predicate tests element names itself, so it is the same
        // whatever compound it hangs off: `optgroup:disabled` is the
        // `@disabled` arm, and an `option` carrying its own `@disabled`
        // inside an enabled optgroup is caught by that arm too.
        assert_eq!(
            h("optgroup:disabled"),
            h("input:disabled").replace("input[", "optgroup[")
        );
        assert_eq!(
            h("option:enabled"),
            h("input:enabled").replace("input[", "option[")
        );
        // Non-overridden dynamic pseudos still never match.
        assert_eq!(h("a:hover"), "a[0]");
        assert_eq!(h("a:visited"), "a[0]");
        assert_eq!(h("a:focus-within"), "a[0]");
        assert_eq!(h("a:focus-visible"), "a[0]");
        // Xhtml shares every HTML pseudo-class override (only name/
        // attribute-value casing differs between the two modes).
        let xhtml = Translator::new(Mode::Xhtml);
        let x = |css: &str| xhtml.css_to_xpath(css, "").unwrap();
        assert_eq!(x("a:link"), h("a:link"));
        assert_eq!(x("input:checked"), h("input:checked"));
        assert_eq!(x("input:required"), h("input:required"));
        assert_eq!(x("select:optional"), h("select:optional"));
        assert_eq!(x("input:disabled"), h("input:disabled"));
        assert_eq!(x("input:enabled"), h("input:enabled"));
        // Form-state pseudo-classes with no exact static translation
        // stay unknown in every mode, HTML included.
        assert!(html.css_to_xpath("input:read-only", "").is_err());
        assert!(html.css_to_xpath("input:read-write", "").is_err());
        assert!(html.css_to_xpath("input:placeholder-shown", "").is_err());
        assert!(html.css_to_xpath("input:default", "").is_err());
        assert!(html.css_to_xpath("input:indeterminate", "").is_err());
    }

    #[test]
    fn html_translator_lowercases_names_not_values() {
        let html = Translator::new(Mode::Html);
        assert_eq!(html.css_to_xpath("DIV", "").unwrap(), "div");
        assert_eq!(html.css_to_xpath("[FOO]", "").unwrap(), "*[@foo]");
        // Names lowercase, values keep their case.
        assert_eq!(
            html.css_to_xpath("DIV[Value=\"Mixed Case\"]", "").unwrap(),
            "div[@value = 'Mixed Case']"
        );
        // The element inside local-name() is lowercased too.
        assert_eq!(
            html.css_to_xpath("*|DIV", "").unwrap(),
            "*[local-name() = 'div']"
        );
        // xhtml preserves case
        let xhtml = Translator::new(Mode::Xhtml);
        assert_eq!(xhtml.css_to_xpath("DIV", "").unwrap(), "DIV");
    }

    /// HTML names are ASCII case-insensitive: the parser lowercases
    /// A-Z and leaves every other code point alone, so full Unicode
    /// case mapping (which turns '\u{130}' into "i\u{307}") would build a
    /// name no document ever has.
    #[test]
    fn html_name_lowercasing_is_ascii_only() {
        let html = Translator::new(Mode::Html);
        assert_eq!(
            html.css_to_xpath("\u{130}", "").unwrap(),
            "*[name() = '\u{130}' and namespace-uri() = '']"
        );
        assert_eq!(
            html.css_to_xpath("[\u{130}]", "").unwrap(),
            "*[attribute::*[name() = '\u{130}']]"
        );
        assert_eq!(
            html.css_to_xpath("*|\u{130}", "").unwrap(),
            "*[local-name() = '\u{130}']"
        );
    }

    #[test]
    fn prefix_applied_per_branch() {
        let t = Translator::new(Mode::Generic);
        assert_eq!(
            t.css_to_xpath("a, b", "descendant-or-self::").unwrap(),
            "descendant-or-self::a | descendant-or-self::b"
        );
    }

    /// css-syntax-3 auto-closes open blocks, functions, and strings at
    /// EOF: the parse error is flagged, not fatal, so a selector left
    /// unclosed at end-of-input translates identically to its closed
    /// form, in every translator mode.
    #[test]
    fn eof_autocloses() {
        fn eof(unclosed: &str, closed: &str) {
            for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
                let t = Translator::new(mode);
                assert_eq!(
                    t.css_to_xpath(unclosed, "").unwrap(),
                    t.css_to_xpath(closed, "").unwrap(),
                    "{unclosed:?} vs {closed:?} in {mode:?}"
                );
            }
        }

        eof("[rel", "[rel]");
        eof("[rel=stylesheet", "[rel=stylesheet]");
        eof("[rel=stylesheet i", "[rel=stylesheet i]");
        eof("[foo=\"bar", "[foo=\"bar\"]");
        eof("[foo=\"", "[foo=\"\"]");
        eof(":lang(fr", ":lang(fr)");
        eof(":nth-child(2n+1", ":nth-child(2n+1)");
        eof(":is(a", ":is(a)");
        eof("e:is(a, b", "e:is(a, b)");
        eof(":not(a", ":not(a)");
        eof(":has(> a", ":has(> a)");
        // The unclosed string is auto-closed at parse time; the
        // pseudo-class is then rejected at translation time either way.
        let t = Translator::new(Mode::Generic);
        assert!(t.css_to_xpath(":contains(\"foo", "").is_err());
    }

    /// `Error::into_message`'s wording — including the caret-pointer
    /// gutter under a `Parse` error, and the plain sentence for an
    /// `Unsupported` one — is documented in `translate::error` as part
    /// of the crate's output contract. Pin it here, plus the `Parse` vs
    /// `Unsupported` variant split, which selects the message shape.
    #[test]
    fn error_messages() {
        let t = Translator::new(Mode::Generic);

        // A dangling combinator: not valid CSS, so `Error::Parse`. The
        // caret lands one past the last character, at the EOF offset.
        let sel = "div > ";
        let err = t.css_to_xpath(sel, "").unwrap_err();
        assert_eq!(err, crate::Error::Parse("DanglingCombinator".to_owned(), 6));
        assert_eq!(
            err.into_message(sel),
            "Unable to parse the CSS selector \"div > \": DanglingCombinator\n\
             \x20 |\n\
             \x20 | div > \n\
             \x20 |       ^"
        );

        // A stray '#' where an attribute value is expected: also a
        // `Parse` error, caret under the offending character.
        let sel = "[foo=#]";
        let err = t.css_to_xpath(sel, "").unwrap_err();
        assert_eq!(
            err,
            crate::Error::Parse("BadValueInAttr(Delim('#'))".to_owned(), 5)
        );
        assert_eq!(
            err.into_message(sel),
            "Unable to parse the CSS selector \"[foo=#]\": BadValueInAttr(Delim('#'))\n\
             \x20 |\n\
             \x20 | [foo=#]\n\
             \x20 |      ^"
        );

        // An invalid character ('/' is not valid CSS syntax here).
        let sel = "html/body";
        let err = t.css_to_xpath(sel, "").unwrap_err();
        assert_eq!(
            err,
            crate::Error::Parse("UnexpectedToken(Delim('/'))".to_owned(), 4)
        );
        assert_eq!(
            err.into_message(sel),
            "Unable to parse the CSS selector \"html/body\": UnexpectedToken(Delim('/'))\n\
             \x20 |\n\
             \x20 | html/body\n\
             \x20 |     ^"
        );

        // The column combinator is valid CSS syntax but has no XPath 1.0
        // translation, so it is `Error::Unsupported` — no caret gutter,
        // since there is no single offending byte position.
        let sel = "col || td";
        let err = t.css_to_xpath(sel, "").unwrap_err();
        assert_eq!(
            err,
            crate::Error::Unsupported("the `||` column combinator".to_owned())
        );
        assert_eq!(
            err.into_message(sel),
            "The CSS selector \"col || td\" uses the `||` column combinator, \
             which this translator does not support"
        );

        // `:scope` inside a functional pseudo-class argument has no
        // reachable context node in an XPath 1.0 predicate: also
        // `Error::Unsupported`, and the only `describe_component` branch
        // reachable through the public API (the other branches all
        // require parser constructs — `::slotted()`, `::part()`, `:host`,
        // `&`, relative-selector scoping — this crate never enables).
        let sel = "e:is(:scope)";
        let err = t.css_to_xpath(sel, "").unwrap_err();
        assert_eq!(
            err,
            crate::Error::Unsupported(
                "the `:scope` pseudo-class inside a functional pseudo-class".to_owned()
            )
        );
        assert_eq!(
            err.into_message(sel),
            "The CSS selector \"e:is(:scope)\" uses the `:scope` pseudo-class \
             inside a functional pseudo-class, which this translator does not support"
        );
    }

    /// The caret gutter's alignment and bounds. The caret is padded by
    /// display width, not by character or byte count, and the gutter
    /// echoes a window of the *line* the error is on, so a message stays
    /// legible (and printable) whatever the selector contains.
    #[test]
    fn error_message_caret_alignment() {
        let t = Translator::new(Mode::Generic);
        // The gutter's echo line and caret line, without the leading
        // `  | ` on each: what has to line up, isolated from the wording.
        let gutter = |sel: &str| {
            let message = t.css_to_xpath(sel, "").unwrap_err().into_message(sel);
            let mut lines = message
                .split('\n')
                .skip(2)
                .map(|l| l["  | ".len()..].to_owned());
            (lines.next().unwrap(), lines.next().unwrap())
        };

        // A tab is echoed as a single space: its rendered width is the
        // terminal's business, and the caret cannot guess the tab stops.
        assert_eq!(
            gutter("\tdiv >"),
            (" div >".to_owned(), "      ^".to_owned())
        );

        // Wide characters take two columns each, so the caret needs
        // eight spaces here and not the five characters (or eleven
        // bytes, or six UTF-16 units) that precede the error.
        assert_eq!(
            gutter("日本語 >"),
            ("日本語 >".to_owned(), "        ^".to_owned())
        );
        // A combining mark takes none, and a non-BMP character — two
        // UTF-16 units — still only one.
        assert_eq!(
            gutter("e\u{301}\u{ff21} >"),
            ("e\u{301}\u{ff21} >".to_owned(), "     ^".to_owned())
        );
        assert_eq!(
            gutter("\u{1f600} >"),
            ("\u{1f600} >".to_owned(), "    ^".to_owned())
        );

        // Control characters are never echoed raw into a terminal.
        assert_eq!(
            gutter("[foo=\u{1}]"),
            ("[foo=\u{fffd}]".to_owned(), "     ^".to_owned())
        );

        // Only the line the error is on is echoed, so the caret's column
        // is a column of the text above it. All of `\n`, `\r`, `\r\n`
        // and `\f` end a line.
        for sel in ["a,\nbbbb >", "a,\rbbbb >", "a,\r\nbbbb >", "a,\u{c}bbbb >"] {
            assert_eq!(gutter(sel), ("bbbb >".to_owned(), "      ^".to_owned()));
        }

        // A line wider than the 72-column window is cut down to it,
        // centred on the caret, with `…` for what was dropped.
        let (line, caret) = gutter(&format!("{}/{}", "a".repeat(100), "b".repeat(100)));
        assert_eq!(line, format!("…{}/{}…", "a".repeat(36), "b".repeat(35)));
        assert_eq!(caret, format!("{}^", " ".repeat(37)));
        assert_eq!(line.chars().count(), 74); // 72 columns plus both `…`
        // The pad counts the leading `…` too, so the caret really is
        // under the `/` as printed.
        assert_eq!(line.chars().nth(caret.len() - 1), Some('/'));

        // Put together: a 20 KB selector still yields a message a caller
        // can print, with the caret intact — the selector is quoted only
        // as far as the 120-byte elision, and echoed only as far as the
        // window.
        let big = format!("{}div >", "a,".repeat(10_000));
        let message = t.css_to_xpath(&big, "").unwrap_err().into_message(&big);
        assert!(message.len() < 1024, "{} bytes", message.len());
        assert!(message.starts_with(&format!(
            "Unable to parse the CSS selector \"{}…\":",
            &big[..120]
        )));
        assert!(message.ends_with(&format!(
            "\n  | …{}div >\n  | {}^",
            "a,".repeat(33),
            " ".repeat(72)
        )));
    }
}
