//! Attribute selectors beyond their basic form: the `i`/`s` case
//! flags, and picking a quote delimiter for the compared value.

mod cases;
use cases::Cases;
use css_to_xpath::Mode;

#[test]
fn case_sensitivity_flags() {
    const LOWER_FOO: &str = "translate(@foo, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                             'abcdefghijklmnopqrstuvwxyz')";
    let mut t = Cases::new(Mode::Generic);
    t.check("e[foo=\"Bar\" i]", format!("e[{LOWER_FOO} = 'bar']"));
    // Flag idents are themselves case-insensitive.
    t.check("e[foo=\"Bar\" I]", format!("e[{LOWER_FOO} = 'bar']"));
    t.check(
        "e[foo^=\"Bar\" i]",
        format!("e[starts-with({LOWER_FOO}, 'bar')]"),
    );
    t.check(
        "e[foo$=\"Bar\" i]",
        format!(
            "e[substring({LOWER_FOO}, \
             string-length({LOWER_FOO})-2) = 'bar']"
        ),
    );
    t.check(
        "e[foo*=\"Bar\" i]",
        format!("e[contains({LOWER_FOO}, 'bar')]"),
    );
    t.check(
        "e[foo~=\"Bar\" i]",
        format!(
            "e[contains(concat(' ', \
             normalize-space({LOWER_FOO}), ' '), ' bar ')]"
        ),
    );
    t.check(
        "e[foo|=\"Bar\" i]",
        format!(
            "e[{LOWER_FOO} = 'bar' or \
             starts-with({LOWER_FOO}, 'bar-')]"
        ),
    );
    // 's' requests default case-sensitive matching on any operator.
    t.check("e[foo^=\"Bar\" s]", "e[starts-with(@foo, 'Bar')]");
    // ASCII-only lowering: non-ASCII characters are left alone.
    t.check(
        "e[foo=\"B\u{e4}r\" i]",
        format!("e[{LOWER_FOO} = 'b\u{e4}r']"),
    );
    // An empty value keeps the exact translation.
    t.check("e[foo=\"\" i]", "e[@foo = '']");
    // 's' requests the default case-sensitive matching.
    t.check("e[foo=\"Bar\" s]", "e[@foo = 'Bar']");
    // The flag composes with namespaced attribute forms.
    t.check(
        "e[*|foo=\"Bar\" i]",
        "e[translate(@*[local-name() = 'foo'], \
         'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz') = 'bar']",
    );
}

/// Attribute values containing quote characters pick a delimiter that
/// avoids escaping, falling back to per-character `concat(...)` when
/// the value contains both.
#[test]
fn quote_escaping() {
    let mut t = Cases::new(Mode::Generic);
    // A value with only apostrophes is wrapped in double quotes.
    t.check("*[aval=\"'\"]", "*[@aval = \"'\"]");
    t.check("*[aval=\"'''\"]", "*[@aval = \"'''\"]");
    // A value with only double quotes is wrapped in single quotes.
    t.check("*[aval='\"']", "*[@aval = '\"']");
    t.check("*[aval='\"\"\"']", "*[@aval = '\"\"\"']");
    // A value with both falls back to concat(), split into maximal
    // runs: apostrophe runs inside double quotes, everything between
    // them inside single quotes.
    t.check("*[aval='\"\\'\"']", "*[@aval = concat('\"',\"'\",'\"')]");
    t.check(
        "*[aval='it\\'s \"q\"']",
        "*[@aval = concat('it',\"'\",'s \"q\"')]",
    );
    t.check(
        "*[aval='a\"b\\'\\'c']",
        "*[@aval = concat('a\"b',\"''\",'c')]",
    );
}
