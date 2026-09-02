//! The public surface: the free function, the constants, and the
//! traits `Translator` and `Mode` implement.

use css_to_xpath::{
    DESCENDANT_OR_SELF, MAX_NESTING_DEPTH, MAX_NTH_OF_BYTES, MAX_NTH_OF_DEPTH, Mode, Translator,
    WHOLE_DOCUMENT, css_to_xpath,
};

/// The public surface a caller can hold on to: a `Translator` is a
/// plain value that reports its own mode, and the prefixes and
/// limits the README quotes are constants rather than literals to
/// copy.
#[test]
fn public_api_surface() {
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        let t = Translator::new(mode);
        assert_eq!(t.mode(), mode);
        assert_eq!(t, Translator::new(mode));
        assert_eq!(format!("{t:?}"), format!("Translator {{ mode: {mode:?} }}"));
    }
    assert_ne!(Translator::new(Mode::Html), Translator::new(Mode::Xhtml));

    assert_eq!(
        css_to_xpath("a", DESCENDANT_OR_SELF, Mode::Generic).unwrap(),
        "descendant-or-self::a"
    );
    assert_eq!(
        css_to_xpath("a", WHOLE_DOCUMENT, Mode::Generic).unwrap(),
        "//a"
    );

    // The limits are the ones the errors quote.
    let deep = format!("{}a{}", ":is(".repeat(65), ")".repeat(65));
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
