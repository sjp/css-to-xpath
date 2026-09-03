//! The public surface: the free function, the constants, and the
//! traits `Translator` and `Mode` implement.

use css_to_xpath::{
    DESCENDANT_OR_SELF, MAX_NESTING_DEPTH, MAX_NTH_OF_BYTES, MAX_NTH_OF_DEPTH, Mode, Translator,
    WHOLE_DOCUMENT, css_to_xpath,
};

/// The public surface a caller can hold on to: a `Translator` is a
/// plain value that reports its own mode and default namespace prefix,
/// and the prefixes and limits the README quotes are constants rather
/// than literals to copy.
#[test]
fn public_api_surface() {
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        let t = Translator::new(mode);
        assert_eq!(t.mode(), mode);
        assert_eq!(t.default_namespace_prefix(), None);
        assert_eq!(t, Translator::new(mode));
        assert_eq!(
            format!("{t:?}"),
            format!("Translator {{ mode: {mode:?}, default_namespace: None }}")
        );
    }
    assert_ne!(Translator::new(Mode::Html), Translator::new(Mode::Xhtml));

    // The default namespace is part of a translator's identity, and the
    // builder takes anything that can become an owned prefix.
    let h = Translator::new(Mode::Xhtml).with_default_namespace_prefix("h");
    assert_eq!(h.default_namespace_prefix(), Some("h"));
    assert_eq!(h.mode(), Mode::Xhtml);
    assert_eq!(
        h,
        Translator::new(Mode::Xhtml).with_default_namespace_prefix(String::from("h"))
    );
    assert_ne!(h, Translator::new(Mode::Xhtml));
    assert_ne!(
        h,
        Translator::new(Mode::Xhtml).with_default_namespace_prefix("x")
    );
    // A translator is reusable, and setting a prefix does not disturb
    // the mode-derived behaviour.
    assert_eq!(h.css_to_xpath("p", "").unwrap(), "h:p");
    assert_eq!(h.css_to_xpath("P", "").unwrap(), "h:P");

    assert_eq!(
        css_to_xpath("a", DESCENDANT_OR_SELF, Mode::Generic).unwrap(),
        "descendant-or-self::a"
    );
    assert_eq!(
        css_to_xpath("a", WHOLE_DOCUMENT, Mode::Generic).unwrap(),
        "//a"
    );

    // The limits are the ones the errors quote.
    let deep = format!("{}a{}", ":is(".repeat(33), ")".repeat(33));
    assert!(
        Translator::new(Mode::Generic)
            .css_to_xpath(&deep, "")
            .unwrap_err()
            .to_string()
            .contains(&MAX_NESTING_DEPTH.to_string())
    );
    let nested_of = format!("{}a{}", ":nth-child(1 of ".repeat(9), ")".repeat(9));
    assert!(
        Translator::new(Mode::Generic)
            .css_to_xpath(&nested_of, "")
            .unwrap_err()
            .to_string()
            .contains(&MAX_NTH_OF_DEPTH.to_string())
    );
    assert_eq!(MAX_NTH_OF_BYTES, 1 << 20);
}
