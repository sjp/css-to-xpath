//! The public surface: the free function, the constants, and the
//! traits `Translator` and `Mode` implement.

use css_to_xpath::{
    DESCENDANT_OR_SELF, MAX_NESTING_DEPTH, MAX_NTH_OF_BYTES, MAX_NTH_OF_DEPTH, Mode,
    ParseModeError, Translator, WHOLE_DOCUMENT, css_to_xpath,
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

/// `Mode` converts to and from the three names a CLI flag or a config
/// file carries them as, so that a caller reading one back does not
/// hand-write the same three-arm `match`. The round trip is exact, and
/// the parse is ASCII case-insensitive but accepts nothing else.
#[test]
fn mode_string_conversions() {
    for mode in [Mode::Generic, Mode::Html, Mode::Xhtml] {
        assert_eq!(mode.to_string(), mode.as_str());
        assert_eq!(mode.as_str().parse(), Ok(mode));
        assert_eq!(mode.as_str().to_uppercase().parse(), Ok(mode));
        assert_eq!(mode.to_string().parse(), Ok(mode));
    }
    assert_eq!(Mode::Generic.as_str(), "generic");
    assert_eq!(Mode::Html.as_str(), "html");
    assert_eq!(Mode::Xhtml.as_str(), "xhtml");
    assert_eq!("XhTmL".parse(), Ok(Mode::Xhtml));

    // Nothing else: not an abbreviation, not a near-miss, and not a
    // name with whitespace around it — trimming is the caller's.
    for bad in ["", "xml", "htm", "generic ", " html", "HTML5", "xhtml\n"] {
        assert_eq!(bad.parse::<Mode>(), Err(ParseModeError));
    }
    assert_eq!(
        ParseModeError.to_string(),
        "expected one of `generic`, `html` or `xhtml`"
    );
    // The error is a `std::error::Error`, so `?` in a `main` reading a
    // flag composes without a wrapper.
    let boxed: Box<dyn std::error::Error> = Box::new(ParseModeError);
    assert_eq!(boxed.to_string(), ParseModeError.to_string());
}

/// The plain translator is the default one: `Mode::Generic`, no default
/// namespace.
#[test]
fn defaults_are_the_plain_translator() {
    assert_eq!(Mode::default(), Mode::Generic);
    assert_eq!(Translator::default(), Translator::new(Mode::Generic));
    assert_eq!(Translator::default().css_to_xpath("A", "").unwrap(), "A");
}
