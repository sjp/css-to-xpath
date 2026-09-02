//! What `Mode::Html` and `Mode::Xhtml` do that `Mode::Generic` does
//! not: name casing, HTML's legacy case-insensitive attribute values,
//! and the HTML pseudo-class overrides.

mod cases;
use cases::Cases;
use css_to_xpath::{Mode, Translator};

/// Selectors 4 defers attribute-value case sensitivity to the document
/// language, and HTML makes a fixed list of legacy attributes
/// (`type`, `rel`, `lang`, `checked`, ...) ASCII case-insensitive. Only
/// `Mode::Html` targets an HTML document, so only it folds them.
#[test]
fn html_case_insensitive_attribute_values() {
    const LOWER_TYPE: &str = "translate(@type, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
                              'abcdefghijklmnopqrstuvwxyz')";
    let mut html = Cases::new(Mode::Html);
    let mut xhtml = Cases::new(Mode::Xhtml);

    html.check(
        "input[type=CHECKBOX]",
        format!("input[{LOWER_TYPE} = 'checkbox']"),
    );
    html.check(
        "a[rel=Stylesheet]",
        "a[translate(@rel, 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', \
         'abcdefghijklmnopqrstuvwxyz') = 'stylesheet']",
    );
    // The attribute name is itself matched case-insensitively, so the
    // classification survives a name written in any case.
    html.check("[TYPE=CHECKBOX]", format!("*[{LOWER_TYPE} = 'checkbox']"));
    // Every operator folds, exactly as under the `i` flag.
    html.check(
        "[type^=Check]",
        format!("*[starts-with({LOWER_TYPE}, 'check')]"),
    );

    // Attributes outside HTML's list keep the case-sensitive default.
    html.check("[data-foo=Bar]", "*[@data-foo = 'Bar']");
    // As does an empty value, whose existence test stays exact.
    html.check("[type='']", "*[@type = '']");

    // The `s` flag asks for case-sensitive matching in every mode.
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        let mut t = Cases::new(mode);
        t.check("[type=CHECKBOX s]", "*[@type = 'CHECKBOX']");
    }

    // XHTML is XML: there these attributes are case-sensitive, and
    // `Mode::Generic` knows nothing of HTML at all.
    for mode in [Mode::Generic, Mode::Xhtml] {
        let mut t = Cases::new(mode);
        t.check("input[type=CHECKBOX]", "input[@type = 'CHECKBOX']");
    }

    // A namespaced attribute is not an HTML attribute, so it never
    // folds — but `[|type]`, which means the same as `[type]`, does.
    html.check("[svg|type=X]", "*[@svg:type = 'X']");
    html.check("[*|type=X]", "*[@*[local-name() = 'type'] = 'X']");
    html.check("[|type=X]", format!("*[{LOWER_TYPE} = 'x']"));

    // The `i` flag still folds where the document language does not.
    xhtml.check(
        "input[type=CHECKBOX i]",
        format!("input[{LOWER_TYPE} = 'checkbox']"),
    );
}

/// The HTML translator's pseudo-class overrides.
#[test]
fn html_pseudo_overrides() {
    let html = Translator::new(Mode::Html);
    let h = |css: &str| html.css_to_xpath(css, "").unwrap();
    // Every override identifies its elements by local name, so a
    // compound that already names the element settles those tests at
    // translation time: only the arm for that name is emitted, and a
    // name outside the pseudo-class's element set leaves `0`.
    //
    // :link is `a`/`area` with an @href; the `link` element has an
    // @href but is not one of the elements HTML matches here.
    assert_eq!(h("a:link"), "a[@href]");
    assert_eq!(h("area:link"), "area[@href]");
    assert_eq!(h("link:link"), "link[0]");
    assert_eq!(h("*|a:link"), "*[local-name() = 'a' and @href]");
    assert_eq!(
        h(":link"),
        "*[@href and (local-name() = 'a' or local-name() = 'area')]"
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
        format!("input[@checked and ({t_lc} = 'checkbox' or {t_lc} = 'radio')]")
    );
    assert_eq!(h("option:checked"), "option[@selected]");
    assert_eq!(h("div:checked"), "div[0]");
    assert_eq!(
        h(":checked"),
        format!(
            "*[(@selected and local-name() = 'option') or \
             (@checked and local-name() = 'input' and \
             ({t_lc} = 'checkbox' or {t_lc} = 'radio'))]"
        )
    );
    // :required/:optional test the @required attribute over the
    // elements it applies to; input types where it has no effect
    // match neither. The seven inert types are tested with one
    // `contains()` against a `|`-delimited list rather than seven
    // comparisons, each of which would repeat the whole fold; the
    // pipe-free guard keeps that exact for a value like
    // type="hidden|range", which is none of the keywords.
    let inert = format!(
        "contains('|hidden|range|color|submit|image|reset|button|', \
         concat('|', {t_lc}, '|')) and not(contains({t_lc}, '|'))"
    );
    assert_eq!(
        h("input:required"),
        format!("input[@required and not({inert})]")
    );
    assert_eq!(h("select:optional"), "select[not(@required)]");
    assert_eq!(h("textarea:required"), "textarea[@required]");
    assert_eq!(h("div:required"), "div[0]");
    let applies = format!(
        "((local-name() = 'input' and not({inert})) or \
         local-name() = 'select' or local-name() = 'textarea')"
    );
    assert_eq!(h(":required"), format!("*[@required and {applies}]"));
    assert_eq!(h(":optional"), format!("*[not(@required) and {applies}]"));
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
    let set = "(local-name() = 'button' or local-name() = 'input' or \
                local-name() = 'select' or local-name() = 'textarea' or \
                local-name() = 'optgroup' or local-name() = 'option' or \
                local-name() = 'fieldset')";
    let fd = "count(ancestor::*[local-name() = 'fieldset'][@disabled]) > \
              count(ancestor::*[local-name() = 'legend']\
              [not(preceding-sibling::*[local-name() = 'legend'])]\
              [parent::*[local-name() = 'fieldset'][@disabled]])";
    let disabled = format!(
        "@disabled or \
         (local-name() = 'option' and parent::*[local-name() = 'optgroup'][@disabled]) or \
         (not(local-name() = 'optgroup' or local-name() = 'option') and {fd})"
    );
    assert_eq!(h(":disabled"), format!("*[{set} and ({disabled})]"));
    assert_eq!(h(":enabled"), format!("*[{set} and not({disabled})]"));
    // A named element keeps only the arm that can apply to it: the
    // fieldset rule for a control, the disabled-parent-optgroup rule
    // for an option, and neither for an optgroup itself.
    assert_eq!(h("input:disabled"), format!("input[@disabled or {fd}]"));
    assert_eq!(h("input:enabled"), format!("input[not(@disabled or {fd})]"));
    let optgroup_disabled = "@disabled or parent::*[local-name() = 'optgroup'][@disabled]";
    assert_eq!(h("option:disabled"), format!("option[{optgroup_disabled}]"));
    assert_eq!(
        h("option:enabled"),
        format!("option[not({optgroup_disabled})]")
    );
    assert_eq!(h("optgroup:disabled"), "optgroup[@disabled]");
    assert_eq!(h("optgroup:enabled"), "optgroup[not(@disabled)]");
    // Hyperlinks and the obsolete `keygen`/`command` are in neither
    // set, so nothing is left of the predicate for them.
    assert_eq!(h("a:enabled"), "a[0]");
    assert_eq!(h("a:disabled"), "a[0]");
    assert_eq!(h("keygen:enabled"), "keygen[0]");
    for css in [":enabled", ":disabled", ":checked"] {
        assert!(!h(css).contains("keygen"), "{css}");
        assert!(!h(css).contains("command"), "{css}");
    }
    assert!(!h(":enabled").contains("@href"));
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
    // Every element name inside an override is matched by
    // local-name(), so the overrides see an XHTML document's
    // namespaced elements: a bare node test (`ancestor::fieldset`)
    // would match nothing under a default namespace, and a
    // qualified-name comparison (`name(.) = 'input'`) nothing under
    // a bound prefix (`<h:input>`) — and the user cannot influence
    // either, since these fragments are not the subject they wrote.
    for css in [
        "*|input:disabled",
        "*|input:enabled",
        "*|option:checked",
        "h|option:checked",
        "*|input:required",
        "*|select:optional",
        "*|a:link",
        ":disabled",
        ":enabled",
        ":checked",
    ] {
        let out = x(css);
        assert!(!out.contains("name(.)"), "{css}: {out}");
        for name in [
            "button", "input", "select", "textarea", "optgroup", "option", "fieldset", "legend",
            "a", "area",
        ] {
            for axis in ["ancestor::", "parent::", "preceding-sibling::", "self::"] {
                assert!(!out.contains(&format!("{axis}{name}")), "{css}: {out}");
            }
        }
    }
    // The namespace-agnostic forms are the ones libxml2 needs to
    // reach a control inside a namespaced `<fieldset disabled>` and
    // an `<h:option selected>` under a bound prefix.
    assert!(x("*|input:disabled").contains(fd));
    assert_eq!(x("h|option:checked"), "h:option[@selected]");
    // A prefixed subject pins the local name the same way a bare one
    // does, so the arms for other names go away there too.
    assert_eq!(
        x("h|input:enabled"),
        format!("h:input[not(@disabled or {fd})]")
    );
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
    let mut html = Cases::new(Mode::Html);
    html.check("DIV", "div");
    html.check("[FOO]", "*[@foo]");
    // Names lowercase, values keep their case.
    html.check("DIV[Value=\"Mixed Case\"]", "div[@value = 'Mixed Case']");
    // The element inside local-name() is lowercased too.
    html.check("*|DIV", "*[local-name() = 'div']");
    // XHTML is XML, so both halves keep their case: the element name,
    // the attribute name, and the value.
    let mut xhtml = Cases::new(Mode::Xhtml);
    xhtml.check("DIV", "DIV");
    xhtml.check("[FOO]", "*[@FOO]");
    xhtml.check("DIV[FOO=Bar]", "DIV[@FOO = 'Bar']");
    xhtml.check("*|DIV", "*[local-name() = 'DIV']");
}

/// HTML names are ASCII case-insensitive: the parser lowercases
/// A-Z and leaves every other code point alone, so full Unicode
/// case mapping (which turns '\u{130}' into "i\u{307}") would build a
/// name no document ever has.
#[test]
fn html_name_lowercasing_is_ascii_only() {
    let mut html = Cases::new(Mode::Html);
    html.check("\u{130}", "*[name() = '\u{130}' and namespace-uri() = '']");
    html.check("[\u{130}]", "*[attribute::*[name() = '\u{130}']]");
    html.check("*|\u{130}", "*[local-name() = '\u{130}']");
}
