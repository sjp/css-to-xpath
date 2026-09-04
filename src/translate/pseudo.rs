//! The non-tree-structural pseudo-class translations: the "never matches"
//! set, the HTML overrides, and `:lang()`/`:dir()`.
//!
//! Both `html` and `xhtml` use the HTML overrides (they differ in the
//! lowercasing flags and in where `:lang()` reads an element's language
//! from); the generic translator answers `0` (never matches) for
//! everything except `:lang()`, which it maps to XPath's `lang()`
//! function.

use crate::parser::PseudoClass;

use super::error::{Error, echoed};
use super::xpath_expr::{Condition, XPathExpr, ascii_lower, xpath_literal};
use super::{Kind, Translator};

/// Where a translator reads an element's language from, for `:lang()`.
/// The two halves — which elements carry a language, and what that
/// element's language string is — are the only things that differ
/// between the flavours, so every `:lang()` condition is built from
/// [`LangSource::nearest`] plus [`LangSource::string`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LangSource {
    /// Generic: `xml:lang`, which is what XPath's own `lang()` reads.
    /// Only the wildcard range needs it spelled out; every other range
    /// goes through `lang()` itself. XML binds the `xml` prefix
    /// implicitly, so it needs no entry in the caller's namespace map —
    /// libxml2 pre-binds it; a processor that resolves prefixes purely
    /// from a caller-supplied map (sxd-xpath, say) needs it registered.
    XmlLang,
    /// `html`: the `lang` content attribute alone. An HTML parser puts a
    /// literal `xml:lang` in no namespace on HTML elements, and HTML's
    /// language determination ignores it, so `@lang` is the whole story.
    Lang,
    /// `xhtml`: either attribute, since XHTML documents conventionally
    /// carry `xml:lang` (often alongside `lang`). HTML's language
    /// determination takes the nearest ancestor-or-self with either one
    /// and prefers `xml:lang` when both sit on that element.
    Both,
}

impl LangSource {
    /// The nearest ancestor-or-self carrying a language attribute (`[1]`
    /// counts backwards along a reverse axis). An element with the
    /// attribute but an empty value still stops the walk: an empty value
    /// resets the language to unknown rather than deferring to a further
    /// ancestor.
    fn nearest(self) -> &'static str {
        match self {
            LangSource::XmlLang => "ancestor-or-self::*[@xml:lang][1]",
            LangSource::Lang => "ancestor-or-self::*[@lang][1]",
            LangSource::Both => "ancestor-or-self::*[@xml:lang or @lang][1]",
        }
    }

    /// That element's language string, as an expression evaluated with
    /// the element as the context node.
    ///
    /// For [`LangSource::Both`], `xml:lang` wins whenever it is present.
    /// XPath 1.0 has no conditional, so the `lang` half is truncated to
    /// zero length (`string-length(@lang) * not(@xml:lang)` multiplies
    /// the length by 0 when `xml:lang` is there) and `concat` contributes
    /// `""` for a missing attribute — leaving exactly one of the two.
    fn string(self) -> &'static str {
        match self {
            LangSource::XmlLang => "@xml:lang",
            LangSource::Lang => "@lang",
            LangSource::Both => {
                "concat(@xml:lang, \
                 substring(@lang, 1, string-length(@lang) * not(@xml:lang)))"
            }
        }
    }
}

/// The HTML `type` attribute, ASCII-lowercased so comparisons against
/// enumerated-attribute keywords are case-insensitive: `type` is an
/// [enumerated attribute](https://html.spec.whatwg.org/multipage/common-microsyntaxes.html#enumerated-attribute),
/// so `type="RADIO"` is a radio and `type="HIDDEN"` is hidden. This is the
/// same ASCII fold `[type=...]` itself gets (`Translator::apply_case_flag`),
/// through the same helper.
fn type_lc() -> String {
    ascii_lower("@type")
}

/// Every element name in the HTML overrides is matched by `local-name()`,
/// never by a qualified name or a bare node test. The overrides are the one
/// part of a translation the caller cannot spell themselves, and the
/// document they run against may put HTML's elements in a namespace: in
/// XHTML they are in `http://www.w3.org/1999/xhtml`, so `ancestor::fieldset`
/// matches nothing, and under a bound prefix (`<h:input>`) a qualified-name
/// comparison against `'input'` fails too. Matching by local name makes the
/// fragments work for `*|input` and `h|input` subjects alike, and leaves
/// `Mode::Html` unchanged in meaning — libxml2's HTML parser produces no
/// namespaces, so there `local-name()` and the qualified name always agree.
/// The crate's namespace rule for *written* names (an unprefixed name is
/// the null namespace) is unaffected: that governs the subject the user
/// writes, which is still translated as documented.
///
/// A form control is disabled by a `fieldset[disabled]` ancestor unless it
/// sits inside that fieldset's first `legend` child (HTML's "actually
/// disabled" carve-out keeps a disabled group's caption usable). Each such
/// first-legend ancestor protects against exactly one disabled fieldset
/// (distinct legends have distinct parents), so the control is
/// fieldset-disabled iff it has more disabled-fieldset ancestors than
/// protecting legends — which counts nested disabled fieldsets correctly.
const FIELDSET_DISABLED: &str = "count(ancestor::*[local-name() = 'fieldset'][@disabled]) > \
     count(ancestor::*[local-name() = 'legend']\
     [not(preceding-sibling::*[local-name() = 'legend'])]\
     [parent::*[local-name() = 'fieldset'][@disabled]])";

/// The elements HTML's `:enabled` and `:disabled` apply to.
/// Form-associated custom elements are in the spec's list too, but
/// nothing in static markup identifies one, so they are left out.
/// Hyperlinks are not in the list — `a[href]` matches `:link`, never
/// `:enabled` — and neither are the obsolete `keygen` and `command`.
///
/// The list is by element and only by element: no `input` type is
/// carved out of it, because HTML's "actually disabled" is defined over
/// these elements and the `disabled` content attribute applies to every
/// `input` type state, Hidden included. So `<input type="hidden"
/// disabled>` is `:disabled` and a hidden input without the attribute is
/// `:enabled`, and neither translation mentions `@type`. That is what
/// separates these two from [`REQUIRED_INERT_TYPES`] and its neighbours,
/// where the type does decide: those attributes have an "Applies to"
/// table listing the type states they affect, and `disabled` has none.
/// Other translators (selectr) drop `type="hidden"` from both halves,
/// which is a carve-out the spec does not make.
const DISABLEABLE: [&str; 7] = [
    "button", "input", "select", "textarea", "optgroup", "option", "fieldset",
];

/// [`DISABLEABLE`] as a condition, for a subject whose local name the
/// compound does not pin.
fn disableable() -> String {
    let names: Vec<String> = DISABLEABLE
        .iter()
        .map(|name| format!("local-name() = '{name}'"))
        .collect();
    format!("({})", names.join(" or "))
}

/// The elements HTML's "nearest ancestor `select`" walk stops at, bar
/// the `select` it is looking for: reaching one of these means there is
/// no nearest ancestor `select`.
const SELECT_WALK_STOPS: &str = "local-name() = 'select' or local-name() = 'datalist' or \
     local-name() = 'hr' or local-name() = 'option'";

/// "Whose nearest ancestor `select` is disabled" — the rule HTML gave
/// both the `option` and the `optgroup` bullet of "actually disabled",
/// so that disabling a `select` disables what it contains.
///
/// The walk climbs the ancestors and gives up at a `datalist`, `hr` or
/// `option`, so this has the shape of the `:lang()` walk (see
/// [`lang_ancestor_condition`]): a reverse-axis `[1]` over those and
/// `select` picks the ancestor that settles the question, and the rest
/// asks whether it is a disabled `select`. An `option` in a
/// `<datalist>` inside a disabled `select` is left out, because the
/// walk stops at the `datalist`.
///
/// The one step the expression cannot take is counting: the algorithm
/// also gives up once it has passed a *second* `optgroup`, so an
/// `option` nested two `optgroup`s deep inside a disabled `select` is
/// disabled here and not in HTML. Nested `optgroup`s are
/// non-conforming, and this is the same kind of approximation as the
/// `legend` counting in [`FIELDSET_DISABLED`] — spelled out rather than
/// paid for in XPath.
fn nearest_select_disabled() -> String {
    format!("ancestor::*[{SELECT_WALK_STOPS}][1][local-name() = 'select'][@disabled]")
}

/// The `optgroup` half of an `option`'s own disabledness: the nearest
/// ancestor among `optgroup` and [`SELECT_WALK_STOPS`] is an `optgroup`
/// carrying `disabled`. HTML widened this from the parent to the
/// nearest such ancestor, because the customizable-`select` markup nests
/// an `option` below its `optgroup` rather than directly under it — so
/// the parent test is no longer the whole rule even in conforming
/// documents.
fn option_disabled_by_optgroup() -> String {
    format!(
        "ancestor::*[local-name() = 'optgroup' or {SELECT_WALK_STOPS}][1]\
         [local-name() = 'optgroup'][@disabled]"
    )
}

/// HTML's "actually disabled", to be read together with [`DISABLEABLE`]:
/// `:disabled` is that set and this condition, `:enabled` is that set and
/// not this condition, so the two stay exact complements.
///
/// An element is actually disabled if it carries `@disabled`; an
/// `option` is disabled by a disabled `optgroup` above it as well; an
/// `option` or an `optgroup` is disabled by a disabled `select` above
/// it; and a control — or a nested `fieldset`, which is itself a
/// disabled fieldset — is disabled by a disabled `fieldset` ancestor.
/// The fieldset rule is the one that reaches neither `optgroup` nor
/// `option`, and the two `select`-side rules are the only ones that do.
fn actually_disabled() -> String {
    let by_optgroup = option_disabled_by_optgroup();
    let by_select = nearest_select_disabled();
    format!(
        "@disabled or \
         (local-name() = 'option' and {by_optgroup}) or \
         ((local-name() = 'option' or local-name() = 'optgroup') and {by_select}) or \
         (not(local-name() = 'optgroup' or local-name() = 'option') \
          and {FIELDSET_DISABLED})"
    )
}

/// The `input` types on which `required` has no effect, as a
/// `|`-delimited haystack: `contains()` against it tests all seven with
/// one mention of `@type`, where seven `=` comparisons would repeat the
/// whole `translate()` fold (see [`type_lc`]) seven times.
const REQUIRED_INERT_TYPES: &str = "|hidden|range|color|submit|image|reset|button|";

/// The `input` types the `readonly` attribute has no effect on, in the
/// same `|`-delimited form. An unrecognised or missing `type` is the
/// Text state, which `readonly` *does* apply to, so the inert types are
/// the ones worth listing: no value outside this list is inert.
const READONLY_INERT_TYPES: &str =
    "|hidden|color|checkbox|radio|file|submit|image|reset|button|range|";

/// The `input` types the `placeholder` attribute has no effect on, same
/// form and same reasoning: `placeholder` applies to Text, Search, URL,
/// Telephone, Email, Password and Number, and to whatever an invalid or
/// missing `type` falls back to (Text).
const PLACEHOLDER_INERT_TYPES: &str = concat!(
    "|hidden|checkbox|radio|file|submit|image|reset|button|color|range|",
    "date|month|week|time|datetime-local|"
);

/// Whether `@type` names one of the keywords in a `|`-delimited
/// haystack, such as [`REQUIRED_INERT_TYPES`].
///
/// `contains(haystack, concat('|', type, '|'))` on its own would also
/// accept a value spelling out several keywords in a row
/// (`type="hidden|range"`), so the pipe-free guard keeps the test exact:
/// a value containing `|` is none of the keywords. The comparison folds
/// case because `type` is an HTML enumerated attribute.
fn type_is_one_of(keywords: &str) -> String {
    let type_lc = type_lc();
    format!(
        "contains('{keywords}', concat('|', {type_lc}, '|')) \
         and not(contains({type_lc}, '|'))"
    )
}

/// Whether `@type` names one of [`REQUIRED_INERT_TYPES`].
fn required_is_inert() -> String {
    type_is_one_of(REQUIRED_INERT_TYPES)
}

/// The elements the `required` attribute applies to, for `:required` and
/// `:optional` (HTML spec): `select`, `textarea`, and `input` except the
/// types on which `required` has no effect — those match neither
/// pseudo-class, whatever attributes they carry. For a subject whose
/// local name the compound does not pin.
fn required_applies() -> String {
    format!(
        "((local-name() = 'input' and not({})) or \
         local-name() = 'select' or \
         local-name() = 'textarea')",
        required_is_inert()
    )
}

/// HTML's "actually disabled" for a form control — everything in
/// [`DISABLEABLE`] bar `optgroup` and `option`, whose own rules the
/// fieldset one does not reach. Shared by `:disabled`/`:enabled` and by
/// the mutability half of `:read-write`.
fn control_actually_disabled() -> String {
    format!("@disabled or {FIELDSET_DISABLED}")
}

/// The `contenteditable` values that *set* a state rather than inheriting
/// one, `|`-delimited for the same `contains()` idiom [`type_is_one_of`]
/// uses on `@type`. The leading `||` admits the empty value, which is the
/// True state; every other value — `inherit`, a typo, anything — leaves
/// the element inheriting its parent's state, so the walk must pass it by.
const CONTENTEDITABLE_STATES: &str = "||true|plaintext-only|false|";

/// Whether the element is editable: the nearest ancestor-or-self that
/// sets a `contenteditable` state sets it to something other than
/// `false`.
///
/// This is the third arm of `:read-write` — "elements that are editing
/// hosts or editable" — and the whole of it for an element outside the
/// form controls. It has the shape of the `:lang()` walk: a reverse-axis
/// `[1]` picks the nearest element that settles the question, and a
/// further predicate asks what it settled it to. `designMode`, the other
/// way a document becomes editable, is not in the markup.
fn editable() -> String {
    let ce_lc = ascii_lower("@contenteditable");
    format!(
        "ancestor-or-self::*[@contenteditable and \
         contains('{CONTENTEDITABLE_STATES}', concat('|', {ce_lc}, '|')) \
         and not(contains(@contenteditable, '|'))][1]\
         [not({ce_lc} = 'false')]"
    )
}

/// `:read-write` for an `input`: `readonly` applies to its type (an
/// invalid or missing `type` is Text, which it applies to), and the
/// control is neither read-only nor disabled.
fn input_mutable() -> String {
    format!(
        "not({}) and not(@readonly) and not({})",
        type_is_one_of(READONLY_INERT_TYPES),
        control_actually_disabled()
    )
}

/// `:read-write` for a `textarea`: no type to consider, so just neither
/// read-only nor disabled.
fn textarea_mutable() -> String {
    format!("not(@readonly) and not({})", control_actually_disabled())
}

/// `:read-write` — a mutable `input` or `textarea`, or an editable
/// element. `:read-only` is Selectors 4's complement of it, so both are
/// built from this one expression and the two partition every element.
fn read_write(name: Option<&str>) -> Condition {
    let editable = editable();
    match name {
        Some("input") => or_group(&format!("({}) or {editable}", input_mutable())),
        Some("textarea") => or_group(&format!("({}) or {editable}", textarea_mutable())),
        // A control is not the only editable thing: any element inside a
        // contenteditable subtree is user-alterable, whatever its name.
        Some(_) => plain(&editable),
        None => or_group(&format!(
            "(local-name() = 'input' and {}) or \
             (local-name() = 'textarea' and {}) or \
             {editable}",
            input_mutable(),
            textarea_mutable()
        )),
    }
}

/// A submit button, as a condition on an element of unknown name: a
/// `button` whose `type` is neither `reset` nor `button` (the missing and
/// invalid value default is Submit), or an `input` of type `submit` or
/// `image`.
fn submit_button() -> String {
    let type_lc = type_lc();
    format!(
        "(local-name() = 'button' and not({type_lc} = 'reset' or {type_lc} = 'button')) or \
         (local-name() = 'input' and ({type_lc} = 'submit' or {type_lc} = 'image'))"
    )
}

/// The tail of `:default`'s first arm, to be read after a test that the
/// element *is* a submit button: that it is its form's default button,
/// the first submit button in tree order whose form owner is that form.
///
/// The form owner is taken to be the nearest ancestor `form`, which is
/// what it is for every control that does not carry a `form` attribute
/// (see the README's Approximations). XPath 1.0 has no node-identity
/// operator, so "the form's first submit button is me" is written with
/// the union-count idiom: `count(A | B) = 1` holds exactly when the two
/// node-sets are the same single node. The ancestor test in front of it
/// is not redundant — with no ancestor form the union would be `.` alone,
/// which also counts 1.
fn is_default_button() -> String {
    format!(
        "ancestor::*[local-name() = 'form'] and \
         count(. | ancestor::*[local-name() = 'form'][1]/descendant::*[{}][1]) = 1",
        submit_button()
    )
}

impl Translator {
    pub(crate) fn apply_pseudo_class(
        &self,
        xpath: &mut XPathExpr,
        pc: &PseudoClass,
    ) -> Result<(), Error> {
        // The HTML overrides name elements through `local-name()`, so
        // when the compound already pins the subject's local name every
        // disjunct written for another name is decided here rather than
        // by the XPath engine: only the arm that can match is emitted,
        // and a name outside the pseudo-class's element set collapses to
        // `0`. `None` — a wildcard subject — keeps the full expression.
        // The element part of a compound is always translated before its
        // conditions, so the name is already known by the time any
        // pseudo-class is applied.
        let name = xpath.local_name.clone();
        let name = name.as_deref();
        match (self.kind(), pc) {
            (_, PseudoClass::Dir(_)) => {
                // :dir() matches by *resolved* directionality, which needs
                // runtime bidi resolution, so it never matches — in both
                // translators. A nearest-@dir-ancestor walk (like the HTML
                // :lang() translation) was considered and rejected: it
                // gets dir="auto" (first-strong-character detection),
                // bdi/form-control defaults, and HTML's invalid-value-
                // means-inherit rule wrong, all of which occur in real
                // markup.
                xpath.add_condition("0");
            }
            (Kind::Generic, PseudoClass::Lang(args)) => {
                self.lang_generic(xpath, args)?;
            }
            (Kind::Html, PseudoClass::Lang(args)) => {
                self.lang_html(xpath, args)?;
            }
            // HTML overrides
            (Kind::Html, PseudoClass::Checked) => {
                xpath.push_condition(checked_condition(name));
            }
            // :any-link is :link ∪ :visited. A static document has no
            // visited state, so every link counts as unvisited and the
            // two pseudo-classes coincide — :any-link shares :link's
            // translation verbatim. HTML matches both on an `a` or
            // `area` with an `href`; the `link` element carries an
            // `href` but is not in the set, and that is the spec's
            // wording, not an omission here: "all `a` elements that
            // have an `href` attribute, and all `area` elements that
            // have an `href` attribute, must match one of :link and
            // :visited" (HTML, Pseudo-classes), with Selectors 4 giving
            // the same two elements as its example of what :any-link
            // matches in HTML. Other translators (selectr) include
            // `link`; they are the ones departing from the text.
            (Kind::Html, PseudoClass::Link) | (Kind::Html, PseudoClass::AnyLink) => {
                xpath.add_condition(&match name {
                    Some("a" | "area") => "@href".to_owned(),
                    Some(_) => "0".to_owned(),
                    None => "@href and (local-name() = 'a' or local-name() = 'area')".to_owned(),
                });
            }
            (Kind::Html, PseudoClass::Required) => {
                xpath.add_condition(&required_condition(name, "@required"));
            }
            (Kind::Html, PseudoClass::Optional) => {
                xpath.add_condition(&required_condition(name, "not(@required)"));
            }
            // `:disabled` and `:enabled` are one expression read two
            // ways, so they always partition the element set.
            (Kind::Html, PseudoClass::Disabled) => {
                xpath.push_condition(disabled_condition(name, /* want_disabled = */ true));
            }
            (Kind::Html, PseudoClass::Enabled) => {
                xpath.push_condition(disabled_condition(name, /* want_disabled = */ false));
            }
            // `:read-write` and `:read-only` are the same trick over a
            // wider set: Selectors 4 defines the latter as the
            // complement of the former, so the two partition *every*
            // element, controls and prose alike.
            (Kind::Html, PseudoClass::ReadWrite) => {
                xpath.push_condition(read_write(name));
            }
            (Kind::Html, PseudoClass::ReadOnly) => {
                // `not(...)` supplies its own grouping, whatever is
                // inside it.
                xpath.add_condition(&format!("not({})", read_write(name).expr));
            }
            (Kind::Html, PseudoClass::Default) => {
                xpath.push_condition(default_condition(name));
            }
            (Kind::Html, PseudoClass::PlaceholderShown) => {
                xpath.push_condition(placeholder_shown_condition(name));
            }
            // Everything else never matches.
            _ => {
                xpath.add_condition("0");
            }
        }
        Ok(())
    }

    /// Generic `:lang()`: XPath's `lang()` does language-range prefix
    /// matching natively, so `en` and `en-*` both become `lang('en')`-style
    /// tests. The two ranges that say something *about* the language
    /// rather than matching one — `*` ("known") and `""` ("not known") —
    /// are what `lang()` cannot express, so they walk the language source
    /// instead.
    fn lang_generic(&self, xpath: &mut XPathExpr, ranges: &[String]) -> Result<(), Error> {
        let mut conditions: Vec<String> = Vec::new();
        for value in ranges {
            check_lang_range(value)?;
            check_wildcard_position(value)?;
            if value.is_empty() {
                conditions.push(lang_unknown_condition(self.lang_source()));
            } else if value == "*" {
                conditions.push(lang_known_condition(self.lang_source()));
            } else if let Some(prefix) = value.strip_suffix("-*") {
                // The trailing '-' goes with the wildcard: lang('en-')
                // would never match, since libxml2 expects the argument
                // itself to end at a subtag boundary.
                conditions.push(format!("lang({})", xpath_literal(prefix)));
            } else {
                conditions.push(format!("lang({})", xpath_literal(value)));
            }
        }
        add_lang_conditions(xpath, &conditions);
        Ok(())
    }

    /// HTML `:lang()`: the language of the nearest ancestor-or-self that
    /// has one (see [`LangSource`]) is matched against the range by RFC
    /// 4647 extended filtering, built out of ASCII-lowercased substring
    /// tests (see [`lang_ancestor_condition`]). A wildcard is accepted in
    /// any subtag position, which is what extended filtering allows.
    fn lang_html(&self, xpath: &mut XPathExpr, ranges: &[String]) -> Result<(), Error> {
        let mut conditions: Vec<String> = Vec::new();
        for value in ranges {
            check_lang_range(value)?;
            if value.is_empty() {
                // Not a filtering range at all: the empty range asks
                // that there be no language to filter.
                conditions.push(lang_unknown_condition(self.lang_source()));
                continue;
            }
            let range = LangRange::parse(value);
            if range.subtags.is_empty() {
                // Nothing but wildcards ("*", "*-*"): the whole test is
                // that the tag has a first subtag to match them against.
                conditions.push(lang_known_condition(self.lang_source()));
            } else {
                conditions.push(lang_ancestor_condition(self.lang_source(), &range));
            }
        }
        add_lang_conditions(xpath, &conditions);
        Ok(())
    }
}

/// `:checked` — a selected `option`, or a checked checkbox/radio `input`.
fn checked_condition(name: Option<&str>) -> Condition {
    let type_lc = type_lc();
    match name {
        Some("option") => plain("@selected"),
        Some("input") => plain(&format!(
            "@checked and ({type_lc} = 'checkbox' or {type_lc} = 'radio')"
        )),
        Some(_) => plain("0"),
        None => or_group(&format!(
            "(@selected and local-name() = 'option') or \
             (@checked and local-name() = 'input' \
             and ({type_lc} = 'checkbox' or {type_lc} = 'radio'))"
        )),
    }
}

/// `:default` — a checked checkbox/radio `input`, a selected `option`,
/// or a form's default submit button. The first two arms are `:checked`
/// read off the same attributes; only the third is new.
fn default_condition(name: Option<&str>) -> Condition {
    let type_lc = type_lc();
    let default_button = is_default_button();
    match name {
        Some("option") => plain("@selected"),
        Some("button") => plain(&format!(
            "not({type_lc} = 'reset' or {type_lc} = 'button') and {default_button}"
        )),
        Some("input") => or_group(&format!(
            "(@checked and ({type_lc} = 'checkbox' or {type_lc} = 'radio')) or \
             (({type_lc} = 'submit' or {type_lc} = 'image') and {default_button})"
        )),
        Some(_) => plain("0"),
        None => or_group(&format!(
            "(@selected and local-name() = 'option') or \
             (@checked and local-name() = 'input' \
             and ({type_lc} = 'checkbox' or {type_lc} = 'radio')) or \
             (({}) and {default_button})",
            submit_button()
        )),
    }
}

/// `:placeholder-shown` — an `input` or `textarea` with a non-empty
/// `placeholder` the type allows, and no value. A document says only
/// what the *initial* value is, so this is the state before any typing
/// (see the README's Approximations).
fn placeholder_shown_condition(name: Option<&str>) -> Condition {
    // `input`'s value is the `value` attribute; a `textarea`'s is its
    // text content, and a missing attribute has string-length 0 too, so
    // one `not(string-length(...))` covers "absent or empty" in both.
    let input = format!(
        "string-length(@placeholder) > 0 and not({}) and not(string-length(@value))",
        type_is_one_of(PLACEHOLDER_INERT_TYPES)
    );
    let textarea = "string-length(@placeholder) > 0 and not(string-length())";
    match name {
        Some("input") => plain(&input),
        Some("textarea") => plain(textarea),
        Some(_) => plain("0"),
        None => or_group(&format!(
            "(local-name() = 'input' and {input}) or \
             (local-name() = 'textarea' and {textarea})"
        )),
    }
}

/// `:required` and `:optional`, which differ only in `attr` — the test on
/// the `required` attribute itself — and share the element set.
fn required_condition(name: Option<&str>, attr: &str) -> String {
    match name {
        Some("select" | "textarea") => attr.to_owned(),
        Some("input") => format!("{attr} and not({})", required_is_inert()),
        Some(_) => "0".to_owned(),
        None => format!("{attr} and {}", required_applies()),
    }
}

/// `:disabled` (`want_disabled`) and `:enabled`, which are the same
/// element set and the same "actually disabled" condition, negated.
///
/// A pinned local name settles both halves: outside [`DISABLEABLE`]
/// neither pseudo-class matches, and inside it the arms of
/// [`actually_disabled`] reduce to the ones written for that name — the
/// disabled-`optgroup` and disabled-`select` rules for an `option`, the
/// disabled-`select` rule alone for an `optgroup`, and the
/// disabled-`fieldset`-ancestor rule for everything the spec applies it
/// to.
fn disabled_condition(name: Option<&str>, want_disabled: bool) -> Condition {
    let Some(name) = name else {
        let (set, actually) = (disableable(), actually_disabled());
        return plain(&if want_disabled {
            format!("{set} and ({actually})")
        } else {
            format!("{set} and not({actually})")
        });
    };
    if !DISABLEABLE.contains(&name) {
        return plain("0");
    }
    let (actually, or_group) = match name {
        "optgroup" => (format!("@disabled or {}", nearest_select_disabled()), true),
        "option" => (
            format!(
                "@disabled or {} or {}",
                option_disabled_by_optgroup(),
                nearest_select_disabled()
            ),
            true,
        ),
        _ => (control_actually_disabled(), true),
    };
    if want_disabled {
        Condition {
            expr: actually,
            or_group,
        }
    } else {
        // `not(...)` supplies its own grouping, whatever is inside it.
        plain(&format!("not({actually})"))
    }
}

/// A condition with no top-level `or`.
fn plain(expr: &str) -> Condition {
    Condition {
        expr: expr.to_owned(),
        or_group: false,
    }
}

/// A condition whose expression has a top-level `or`.
fn or_group(expr: &str) -> Condition {
    Condition {
        expr: expr.to_owned(),
        or_group: true,
    }
}

/// Reject a `:lang()` argument that no translator can take. What is left
/// is a language range: one or more non-empty `-`-separated subtags, each
/// either a whole `*` or free of `*` entirely (RFC 4647
/// extended-language-range, minus its restrictions on subtag length and
/// character set — which cost nothing but never-matching output, unlike
/// the shapes rejected here).
///
/// The empty argument passes without being one: it is not a range to
/// filter with but a statement about the language, Level 4's "the
/// element's language is not known" (see [`lang_unknown_condition`]) —
/// the complement of what `:lang(*)` asks.
///
/// The wildcard rule is what keeps a typo like `:lang(en*)` an error
/// rather than the range `en` widened by a `*` that would match every
/// element with a known language. The non-empty-subtag rule rejects
/// `en-` and `--x`; a trailing `-` in particular reads as a
/// half-written `en-*`.
///
/// The check lives here rather than in the parser's argument grammar —
/// which only asks whether the tokens assemble into ranges at all — so
/// that the message can name the offending range, as the wildcard-
/// position one below does. What is left to the grammar is what only the
/// token stream knows: the adjacency that separates `en-*` from `en-`,
/// `*`.
fn check_lang_range(range: &str) -> Result<(), Error> {
    if range.is_empty() {
        return Ok(()); // no subtags to be wrong about
    }
    for subtag in range.split('-') {
        let fault = if subtag.is_empty() {
            "an empty subtag, which a language range cannot have"
        } else if subtag != "*" && subtag.contains('*') {
            "a wildcard glued to a subtag, \
             where a language range takes one only as a whole subtag"
        } else {
            continue;
        };
        return Err(Error::unsupported(format!(
            "the :lang() language range {:?} ({fault})",
            echoed(range)
        )));
    }
    Ok(())
}

/// Under `Mode::Generic` a wildcard is accepted only as a whole range
/// (`*`) or as the final subtag (`en-*`), the two shapes XPath's `lang()`
/// can be handed. RFC 4647 extended filtering also allows one in any
/// other position (`*-CH`, `de-*-DE`): `lang()` does a prefix match and
/// has nowhere to put such a range, so it is rejected rather than
/// silently over- or under-matching. The HTML modes build the comparison
/// themselves and take a wildcard anywhere (see [`LangRange`]).
///
/// ([`check_lang_range`] has already rejected a `*` that is not a whole
/// subtag, such as `en*`, so a range that passes here is either `*`
/// itself or ends in `-*`.)
fn check_wildcard_position(range: &str) -> Result<(), Error> {
    if let Some(pos) = range.find('*')
        && pos != range.len() - 1
    {
        return Err(Error::unsupported(format!(
            "the :lang() language range {:?} \
             (a wildcard outside the final subtag, \
             which XPath's lang() cannot express)",
            echoed(range)
        )));
    }
    Ok(())
}

/// The shared condition-combining tail of both `:lang()` translations: a
/// single condition is added as-is, multiple are OR-joined.
fn add_lang_conditions(xpath: &mut XPathExpr, conditions: &[String]) {
    match conditions.len() {
        0 => {}
        1 => xpath.add_condition(&conditions[0]),
        _ => xpath.add_or_condition(&conditions.join(" or ")),
    }
}

/// The wildcard range `*`, which matches an element whose language is
/// known. The language comes from the nearest ancestor-or-self carrying a
/// language attribute, and an empty value there resets the language to
/// unknown rather than deferring to a further ancestor — so the nearest
/// one must also be non-empty.
fn lang_known_condition(source: LangSource) -> String {
    format!(
        "{}[string-length({}) > 0]",
        source.nearest(),
        source.string()
    )
}

/// The empty range `""`, which Level 4 defines as the complement of `*`:
/// it matches an element whose language is *not* known. That is exactly
/// the absence of the node [`lang_known_condition`] looks for — no
/// ancestor-or-self carries a language attribute, or the nearest one that
/// does carries an empty value — so the test is that condition negated.
fn lang_unknown_condition(source: LangSource) -> String {
    format!("not({})", lang_known_condition(source))
}

/// The element's language as the comparisons below want it: ASCII-folded
/// and dash-terminated, so every subtag — including the last — is
/// bounded by a `-` on the right.
fn folded_lang(source: LangSource) -> String {
    format!("concat({}, '-')", ascii_lower(source.string()))
}

/// A `:lang()` language range reduced to what the match actually tests.
///
/// RFC 4647 gives the range's first subtag and the rest different jobs,
/// and a `*` subtag a different job again, so the range is split along
/// both lines before any XPath is built.
struct LangRange {
    /// Whether the range's first subtag is `*`, which stands in for the
    /// tag's first subtag rather than being compared to it: the tag must
    /// have one, and it may be anything.
    wildcard_head: bool,
    /// The range's literal (non-`*`) subtags in order, ASCII-lowercased
    /// to meet the folded language string. A later `*` contributes
    /// nothing to match — see [`LangRange::parse`] — so none is kept.
    subtags: Vec<String>,
}

impl LangRange {
    /// Split a range into [`LangRange`]'s two halves.
    ///
    /// Only a *leading* `*` consumes a subtag of the tag. RFC 4647's
    /// step 3 pairs the two first subtags and a `*` there matches
    /// anything; every later range subtag is then searched for through
    /// what is left of the tag, and step 4.A moves past a `*` without
    /// touching the tag at all. A `*` after the first position is
    /// therefore a no-op — `de-*-DE` tests exactly what `de-DE` does,
    /// `en-*` exactly what `en` does — and is dropped here.
    ///
    /// [`check_lang_range`] has already rejected a `*` that is not a
    /// whole subtag (`en*`) and an empty subtag (`en-`), and the empty
    /// range is answered before the split (see
    /// [`Translator::lang_html`]), so every subtag reaching this is
    /// either `*` or a non-empty literal.
    fn parse(range: &str) -> Self {
        let mut subtags = range.split('-').map(str::to_ascii_lowercase);
        let head = subtags.next().expect("split is non-empty");
        let wildcard_head = head == "*";
        let mut literals: Vec<String> = if wildcard_head {
            Vec::new()
        } else {
            vec![head]
        };
        literals.extend(subtags.filter(|subtag| subtag != "*"));
        Self {
            wildcard_head,
            subtags: literals,
        }
    }
}

/// The nearest-ancestor language test, as RFC 4647 extended filtering
/// over the folded language string: the language's first subtag must
/// equal the range's first, and each later range subtag must appear as a
/// whole subtag after the previous one matched.
///
/// The chain walks the language string with `substring-after`, keeping
/// the remainder dash-bounded on both ends so `-de-` can only match a
/// whole subtag. Each step takes the *earliest* remaining occurrence,
/// which is the greedy choice that leaves the longest tail, so it finds a
/// match whenever one exists. A subtag that is absent makes
/// `substring-after` return `''`, and every later `contains` is then
/// false — the right answer.
///
/// A [`LangRange::wildcard_head`] range (`*-CH`) starts the walk one
/// subtag in instead of anchoring: the leading `*` stands for the tag's
/// first subtag, whatever it is, so that subtag is dropped and the
/// literals are searched for in what follows. `*-de` therefore matches
/// `gsw-de` and not `de` — the tag's first subtag is spent on the `*`.
/// A range of nothing but wildcards has no literals left to search for
/// and does not reach here (see [`Translator::lang_html`]).
///
/// A single-subtag range is just the `starts-with`, which is both the
/// whole of extended filtering for that shape and the dash-terminated
/// prefix match the translation has always emitted. The one RFC rule the
/// chain does not model is that a subtag may not be skipped past a
/// *singleton* (a one-character subtag, such as the `x` opening a
/// private-use section): measuring the length of every skipped subtag is
/// not expressible in XPath 1.0, so `:lang(de-DE)` also matches
/// `de-x-de`. See the README's Approximations.
fn lang_ancestor_condition(source: LangSource, range: &LangRange) -> String {
    let lang = folded_lang(source);
    let mut subtags = range.subtags.iter();
    let mut conditions: Vec<String> = Vec::new();

    // The first subtag is an equality, which on the dash-terminated
    // string is a prefix match — unless a `*` stands in its place, which
    // asks only that there be a subtag there to skip.
    let mut tail = if range.wildcard_head {
        format!("substring-after({lang}, '-')")
    } else {
        let first = xpath_literal(&format!(
            "{}-",
            subtags
                .next()
                .expect("no literals is the known-language test")
        ));
        conditions.push(format!("starts-with({lang}, {first})"));
        format!("substring-after({lang}, {first})")
    };
    // Everything after it is a whole-subtag search through the tail.
    for subtag in subtags {
        let needle = xpath_literal(&format!("-{subtag}-"));
        let bounded = format!("concat('-', {tail})");
        conditions.push(format!("contains({bounded}, {needle})"));
        tail = format!("substring-after({bounded}, {needle})");
    }

    format!("{}[{}]", source.nearest(), conditions.join(" and "))
}
