//! The reference matcher: Servo's `selectors` matching engine driven
//! over a fixture tree, the second source of truth the differential
//! suites compare the translation against.
//!
//! The tree is copied into a flat arena in document order rather than
//! matched over `sxd_document`'s handles directly. Two reasons: an
//! `Element` must hand out a stable `OpaqueElement` for identity
//! comparisons (`:has()`'s anchor, the nth-index cache), and sxd's
//! element handles are `Copy` values with no publicly reachable address;
//! and the sibling/child navigation the trait wants is a plain index
//! lookup here rather than a scan that skips non-element nodes.
//!
//! Pulled in by both differential binaries with `mod reference;`, so any
//! single binary uses only part of it; hence the blanket `dead_code`
//! allow.
#![allow(dead_code)]

use std::borrow::Borrow;
use std::fmt;

use cssparser::{Parser as CssParser, ParserInput, ToCss};
use precomputed_hash::PrecomputedHash;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::{ElementSelectorFlags, matches_selector};
use selectors::parser::{
    NonTSPseudoClass, ParseRelative, PseudoElement, SelectorImpl, SelectorList,
    SelectorParseErrorKind,
};
use selectors::{Element, OpaqueElement};
use sxd_document::dom::{ChildOfElement, ChildOfRoot, Document, Element as SxdElement};

/// CSS white space, which is what a `class` attribute and `[a~=v]`
/// split on. (Note the form feed, which XPath's `normalize-space`
/// does not treat as white space — a divergence the README records.)
const CSS_WHITESPACE: [char; 5] = [' ', '\t', '\r', '\n', '\u{c}'];

/// A string-ish associated type: identifiers, local names,
/// attribute values and namespace prefixes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Name(String);

impl<'a> From<&'a str> for Name {
    fn from(s: &'a str) -> Self {
        Name(s.to_owned())
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Name {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl ToCss for Name {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_identifier(&self.0, dest)
    }
}

impl PrecomputedHash for Name {
    fn precomputed_hash(&self) -> u32 {
        // Only the bloom filters use this, and matching here runs
        // without one; a constant is consistent with Eq.
        0
    }
}

/// A namespace URL. Unlike the translator's, this one is a real URL,
/// resolved from the fixture's prefix bindings, so that matching can
/// compare it against a document node's namespace. The empty URL is
/// "no namespace", which is also the declared default namespace —
/// see the module comment on unprefixed type selectors.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NsUrl(String);

impl<'a> From<&'a str> for NsUrl {
    fn from(s: &'a str) -> Self {
        NsUrl(s.to_owned())
    }
}

impl Borrow<str> for NsUrl {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl ToCss for NsUrl {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        cssparser::serialize_string(&self.0, dest)
    }
}

impl PrecomputedHash for NsUrl {
    fn precomputed_hash(&self) -> u32 {
        0
    }
}

/// The pseudo-element type. The reference answers nothing that is not
/// in the document tree, and the parser below keeps the trait's
/// rejecting default for every pseudo-element name, so no value of this
/// type is ever constructed. (An uninhabited type would say that
/// outright, but a reference to one cannot be matched without tripping
/// `uninhabited_references`.)
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Never;

impl Never {
    fn unreachable() -> ! {
        unreachable!("the reference parser accepts no pseudo-element")
    }
}

impl ToCss for Never {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        Never::unreachable()
    }
}

impl PseudoElement for Never {
    type Impl = RefImpl;
}

/// The non-tree-structural pseudo-classes the reference can answer:
/// HTML's static ones, whose answer is wholly in the document tree.
///
/// A [`RefParser`] built by [`Reference::new`] accepts none of them —
/// the generic differential suite has no way to answer them — so this
/// set is empty in everything but the HTML-mode suite, which builds its
/// parser with [`Reference::new_html`].
///
/// `:lang()` is deliberately absent: it is the one HTML translation
/// that knowingly diverges from the spec (RFC 4647's singleton rule),
/// so a reference for it would have to encode the divergence rather
/// than check it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PseudoClass {
    Disabled,
    Enabled,
    Checked,
    Required,
    Optional,
    ReadOnly,
    ReadWrite,
    Default,
    PlaceholderShown,
    /// `:link` and `:any-link`, which coincide in a static document:
    /// nothing in the tree says a link has been visited.
    Link,
}

impl PseudoClass {
    /// The pseudo-class this name selects, or `None` for a name the
    /// reference cannot answer.
    fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "disabled" => PseudoClass::Disabled,
            "enabled" => PseudoClass::Enabled,
            "checked" => PseudoClass::Checked,
            "required" => PseudoClass::Required,
            "optional" => PseudoClass::Optional,
            "read-only" => PseudoClass::ReadOnly,
            "read-write" => PseudoClass::ReadWrite,
            "default" => PseudoClass::Default,
            "placeholder-shown" => PseudoClass::PlaceholderShown,
            "link" | "any-link" => PseudoClass::Link,
            _ => return None,
        })
    }
}

impl ToCss for PseudoClass {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(match *self {
            PseudoClass::Disabled => ":disabled",
            PseudoClass::Enabled => ":enabled",
            PseudoClass::Checked => ":checked",
            PseudoClass::Required => ":required",
            PseudoClass::Optional => ":optional",
            PseudoClass::ReadOnly => ":read-only",
            PseudoClass::ReadWrite => ":read-write",
            PseudoClass::Default => ":default",
            PseudoClass::PlaceholderShown => ":placeholder-shown",
            PseudoClass::Link => ":link",
        })
    }
}

impl NonTSPseudoClass for PseudoClass {
    type Impl = RefImpl;

    fn is_active_or_hover(&self) -> bool {
        false
    }

    fn is_user_action_state(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RefImpl;

impl SelectorImpl for RefImpl {
    type ExtraMatchingData<'a> = ();
    type AttrValue = Name;
    type Identifier = Name;
    type LocalName = Name;
    type NamespaceUrl = NsUrl;
    type NamespacePrefix = Name;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = PseudoClass;
    type PseudoElement = Never;
}

/// The selector parser. Only the hooks that widen the grammar are
/// overridden; everything else — pseudo-elements, `:hover` and the
/// rest of the non-tree-structural set — keeps the trait's default,
/// which is to reject.
struct RefParser {
    /// Prefix to namespace URL, as the fixture declares them.
    namespaces: Vec<(String, String)>,
    /// Whether HTML's static pseudo-classes ([`PseudoClass`]) are part
    /// of the grammar. The generic suite leaves them rejected, so a
    /// selector naming one there fails to parse rather than being
    /// answered by a matcher that cannot answer it.
    html: bool,
}

impl<'i> selectors::parser::Parser<'i> for RefParser {
    type Impl = RefImpl;
    type Error = SelectorParseErrorKind<'i>;

    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn is_is_alias(&self, name: &str) -> bool {
        name.eq_ignore_ascii_case("matches")
    }

    fn parse_has(&self) -> bool {
        true
    }

    fn parse_nth_child_of(&self) -> bool {
        true
    }

    /// HTML's static pseudo-classes, and only when this parser is the
    /// HTML one. Everything else — pseudo-elements, `:hover`, `:visited`
    /// and the rest of the set whose answer is not in the tree — keeps
    /// the trait's rejecting default.
    fn parse_non_ts_pseudo_class(
        &self,
        location: cssparser::SourceLocation,
        name: cssparser::CowRcStr<'i>,
    ) -> Result<PseudoClass, cssparser::ParseError<'i, SelectorParseErrorKind<'i>>> {
        match PseudoClass::parse(&name).filter(|_| self.html) {
            Some(pc) => Ok(pc),
            None => Err(location.new_custom_error(
                SelectorParseErrorKind::UnsupportedPseudoClassOrElement(name),
            )),
        }
    }

    fn namespace_for_prefix(&self, prefix: &Name) -> Option<NsUrl> {
        self.namespaces
            .iter()
            .find(|(p, _)| *p == prefix.0)
            .map(|(_, url)| NsUrl(url.clone()))
    }

    /// No default namespace, which is CSS's own reading of a
    /// stylesheet that declares none: an unprefixed type selector
    /// matches its name in any namespace, and so does `*`. The
    /// translator instead reads an unprefixed type selector as the
    /// null namespace, which is why the grammar writes `|e` rather
    /// than `e` — see the module comment.
    fn default_namespace(&self) -> Option<NsUrl> {
        None
    }
}

struct Attr {
    namespace: String,
    local: String,
    value: String,
}

/// One element, flattened out of the document.
struct Node {
    /// How the node is reported, matching the XPath harness: its id.
    label: String,
    namespace: String,
    local: String,
    id: Option<String>,
    classes: Vec<String>,
    attributes: Vec<Attr>,
    parent: Option<usize>,
    /// Element children only, in document order.
    children: Vec<usize>,
    /// This node's index within its parent's `children`.
    sibling_index: usize,
    empty: bool,
    /// The element's string value — every descendant text node, in
    /// order — which is a `textarea`'s value.
    text: String,
}

/// The elements of one document, in document order.
struct Dom {
    nodes: Vec<Node>,
}

impl Dom {
    fn build(document: Document<'_>) -> Self {
        let mut dom = Dom { nodes: Vec::new() };
        for child in document.root().children() {
            if let ChildOfRoot::Element(element) = child {
                dom.push(element, None);
            }
        }
        dom
    }

    fn push(&mut self, element: SxdElement<'_>, parent: Option<usize>) {
        let index = self.nodes.len();
        let name = element.name();
        let attributes: Vec<Attr> = element
            .attributes()
            .into_iter()
            .map(|attribute| Attr {
                namespace: attribute.name().namespace_uri().unwrap_or("").to_owned(),
                local: attribute.name().local_part().to_owned(),
                value: attribute.value().to_owned(),
            })
            .collect();
        let id = element.attribute_value("id").map(ToOwned::to_owned);
        let classes = element.attribute_value("class").map_or_else(Vec::new, |c| {
            c.split(CSS_WHITESPACE)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        });
        self.nodes.push(Node {
            label: id
                .clone()
                .unwrap_or_else(|| format!("<{} with no id>", name.local_part())),
            namespace: name.namespace_uri().unwrap_or("").to_owned(),
            local: name.local_part().to_owned(),
            id,
            classes,
            attributes,
            parent,
            children: Vec::new(),
            sibling_index: parent.map_or(0, |p| self.nodes[p].children.len()),
            // Selectors Level 3: no element children and no
            // non-empty text, which is the translation's reading too.
            empty: element.children().into_iter().all(|child| match child {
                ChildOfElement::Element(_) => false,
                ChildOfElement::Text(text) => text.text().is_empty(),
                _ => true,
            }),
            text: string_value(element),
        });
        if let Some(parent) = parent {
            self.nodes[parent].children.push(index);
        }
        for child in element.children() {
            if let ChildOfElement::Element(child) = child {
                self.push(child, Some(index));
            }
        }
    }
}

/// An element's string value: the text of every descendant text node,
/// in document order. A `textarea`'s value is its child text, which is
/// what `:placeholder-shown` asks about.
fn string_value(element: SxdElement<'_>) -> String {
    let mut out = String::new();
    for child in element.children() {
        match child {
            ChildOfElement::Element(child) => out.push_str(&string_value(child)),
            ChildOfElement::Text(text) => out.push_str(text.text()),
            _ => {}
        }
    }
    out
}

/// A handle onto one element of a [`Dom`].
#[derive(Clone, Copy)]
struct El<'a> {
    dom: &'a Dom,
    index: usize,
}

impl fmt::Debug for El<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} id={}>", self.node().local, self.node().label)
    }
}

impl<'a> El<'a> {
    fn node(&self) -> &'a Node {
        &self.dom.nodes[self.index]
    }

    fn at(&self, index: usize) -> El<'a> {
        El {
            dom: self.dom,
            index,
        }
    }

    fn sibling(&self, offset: isize) -> Option<El<'a>> {
        let node = self.node();
        let parent = &self.dom.nodes[node.parent?];
        let index = node.sibling_index.checked_add_signed(offset)?;
        parent.children.get(index).map(|&index| self.at(index))
    }
}

/// The XHTML namespace, where an XHTML document's HTML elements live.
///
/// HTML's pseudo-classes are defined over HTML *elements*, so every
/// element-name test below pins it. The translation matches by
/// `local-name()` alone, so that its fragments work for a `*|input` or
/// `h|input` subject as well — a deliberate difference the HTML-mode
/// suite pins as a named divergence rather than generating around.
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";

/// An `input` element's [type state][types]. `type` is an enumerated
/// attribute, so its keywords are matched ASCII case-insensitively, and
/// both the missing value default and the invalid value default are
/// Text.
///
/// [types]: https://html.spec.whatwg.org/multipage/input.html#attr-input-type
#[derive(Clone, Copy, PartialEq, Eq)]
enum InputType {
    Hidden,
    Text,
    Search,
    Tel,
    Url,
    Email,
    Password,
    Date,
    Month,
    Week,
    Time,
    DatetimeLocal,
    Number,
    Range,
    Color,
    Checkbox,
    Radio,
    File,
    Submit,
    Image,
    Reset,
    Button,
}

impl InputType {
    fn parse(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return InputType::Text;
        };
        match value.to_ascii_lowercase().as_str() {
            "hidden" => InputType::Hidden,
            "text" => InputType::Text,
            "search" => InputType::Search,
            "tel" => InputType::Tel,
            "url" => InputType::Url,
            "email" => InputType::Email,
            "password" => InputType::Password,
            "date" => InputType::Date,
            "month" => InputType::Month,
            "week" => InputType::Week,
            "time" => InputType::Time,
            "datetime-local" => InputType::DatetimeLocal,
            "number" => InputType::Number,
            "range" => InputType::Range,
            "color" => InputType::Color,
            "checkbox" => InputType::Checkbox,
            "radio" => InputType::Radio,
            "file" => InputType::File,
            "submit" => InputType::Submit,
            "image" => InputType::Image,
            "reset" => InputType::Reset,
            "button" => InputType::Button,
            // The invalid value default.
            _ => InputType::Text,
        }
    }

    /// The type states HTML's `required` attribute applies to, read off
    /// the attribute table's "Applies to" row.
    fn required_applies(self) -> bool {
        matches!(
            self,
            InputType::Text
                | InputType::Search
                | InputType::Url
                | InputType::Tel
                | InputType::Email
                | InputType::Password
                | InputType::Date
                | InputType::Month
                | InputType::Week
                | InputType::Time
                | InputType::DatetimeLocal
                | InputType::Number
                | InputType::Checkbox
                | InputType::Radio
                | InputType::File
        )
    }

    /// The type states `readonly` applies to. Narrower than
    /// [`InputType::required_applies`] by the four types that have no
    /// text to protect: checkbox, radio, file and — since there is
    /// nothing to type — nothing else.
    fn readonly_applies(self) -> bool {
        matches!(
            self,
            InputType::Text
                | InputType::Search
                | InputType::Url
                | InputType::Tel
                | InputType::Email
                | InputType::Password
                | InputType::Date
                | InputType::Month
                | InputType::Week
                | InputType::Time
                | InputType::DatetimeLocal
                | InputType::Number
        )
    }

    /// The type states `placeholder` applies to: the free-text ones
    /// alone, so not the date and time family.
    fn placeholder_applies(self) -> bool {
        matches!(
            self,
            InputType::Text
                | InputType::Search
                | InputType::Url
                | InputType::Tel
                | InputType::Email
                | InputType::Password
                | InputType::Number
        )
    }
}

/// HTML's static pseudo-classes, written from the standard's own
/// definitions — "actually disabled", the attribute tables' "Applies
/// to" rows, the form owner and default button algorithms — rather than
/// from the translator's `pseudo.rs`. A reference transcribed from the
/// code under test would agree with it for free; two readings of the
/// same spec agreeing is the signal worth having.
///
/// Three of the crate's documented approximations are not checkable
/// against a document at all, so they are restated here as the same
/// approximation and named where they are made: `:checked` reads
/// attributes rather than checkedness, `:placeholder-shown` answers for
/// the initial value, and `:default` takes the form owner to be the
/// nearest ancestor `form`.
impl<'a> El<'a> {
    /// This element's ancestors, nearest first.
    fn ancestors(self) -> impl Iterator<Item = El<'a>> {
        std::iter::successors(self.parent_el(), |el| el.parent_el())
    }

    /// This element and its ancestors, nearest first — the walk the
    /// inherited states (`contenteditable`) resolve along.
    fn self_and_ancestors(self) -> impl Iterator<Item = El<'a>> {
        std::iter::once(self).chain(self.ancestors())
    }

    fn parent_el(&self) -> Option<El<'a>> {
        self.node().parent.map(|index| self.at(index))
    }

    fn is_descendant_of(self, other: El<'_>) -> bool {
        self.ancestors().any(|el| el.index == other.index)
    }

    /// Whether this is the HTML element `local`.
    fn is_html(&self, local: &str) -> bool {
        self.node().local == local && self.node().namespace == XHTML_NS
    }

    /// The value of a content attribute — one with no namespace, which
    /// is where HTML's attributes live in an XHTML document too.
    fn attr(&self, local: &str) -> Option<&'a str> {
        self.node()
            .attributes
            .iter()
            .find(|attribute| attribute.local == local && attribute.namespace.is_empty())
            .map(|attribute| attribute.value.as_str())
    }

    fn has_attr(&self, local: &str) -> bool {
        self.attr(local).is_some()
    }

    /// This `input`'s type state.
    fn input_type(&self) -> InputType {
        InputType::parse(self.attr("type"))
    }

    /// The elements `:enabled` and `:disabled` apply to. (The spec's
    /// list also holds form-associated custom elements, which nothing
    /// in static markup identifies.)
    fn is_disableable(&self) -> bool {
        [
            "button", "input", "select", "textarea", "optgroup", "option", "fieldset",
        ]
        .iter()
        .any(|name| self.is_html(name))
    }

    /// The fieldset half of HTML's disabled rules: an element is
    /// disabled by a `fieldset` ancestor whose `disabled` attribute is
    /// specified, unless it is a descendant of that fieldset's first
    /// `legend` element child — the carve-out that keeps a disabled
    /// group's caption usable. Each disabled fieldset ancestor is asked
    /// separately, so a control protected by one can still be disabled
    /// by another further up.
    fn disabled_by_fieldset(self) -> bool {
        self.ancestors().any(|fieldset| {
            fieldset.is_html("fieldset")
                && fieldset.has_attr("disabled")
                && !fieldset
                    .first_legend_child()
                    .is_some_and(|legend| self.is_descendant_of(legend))
        })
    }

    fn first_legend_child(self) -> Option<El<'a>> {
        self.node()
            .children
            .iter()
            .map(|&index| self.at(index))
            .find(|child| child.is_html("legend"))
    }

    /// HTML's ["actually disabled"][ad] concept: the `disabled`
    /// attribute, plus the two inherited rules — an `option` is
    /// disabled by a disabled parent `optgroup`, and a form control or
    /// nested `fieldset` by a disabled `fieldset` ancestor. Neither
    /// inherited rule reaches an `optgroup`, and the fieldset rule does
    /// not reach an `option`.
    ///
    /// [ad]: https://html.spec.whatwg.org/multipage/semantics-other.html#concept-element-disabled
    fn is_actually_disabled(self) -> bool {
        if self.has_attr("disabled") && self.is_disableable() {
            return true;
        }
        if self.is_html("option") {
            return self
                .parent_el()
                .is_some_and(|parent| parent.is_html("optgroup") && parent.has_attr("disabled"));
        }
        if self.is_html("optgroup") {
            return false;
        }
        self.is_disableable() && self.disabled_by_fieldset()
    }

    /// The `contenteditable` state an element *sets*, if any: `true`
    /// (the empty string too) and `plaintext-only` make it an editing
    /// host, `false` makes it plainly not one, and `inherit` — like any
    /// other value, since `contenteditable` is an enumerated attribute
    /// whose invalid value default is the inherit state — sets nothing.
    fn contenteditable_state(&self) -> Option<bool> {
        match self.attr("contenteditable")?.to_ascii_lowercase().as_str() {
            "" | "true" | "plaintext-only" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Whether the element is an editing host or editable: the nearest
    /// ancestor-or-self that sets a `contenteditable` state sets it to
    /// something other than `false`. (`designMode`, the other way a
    /// document becomes editable, is not in the markup — the crate's
    /// documented approximation, and the same one here.)
    fn is_editable(self) -> bool {
        self.self_and_ancestors()
            .find_map(|el| el.contenteditable_state())
            .unwrap_or(false)
    }

    /// Selectors 4's `:read-write` as HTML defines the set: an `input`
    /// the `readonly` attribute applies to that is neither read-only
    /// nor disabled, a `textarea` that is neither, and any element that
    /// is an editing host or editable. `:read-only` is its complement
    /// over every element.
    fn is_read_write(self) -> bool {
        if self.is_editable() {
            return true;
        }
        if self.is_html("input") {
            return self.input_type().readonly_applies()
                && !self.has_attr("readonly")
                && !self.is_actually_disabled();
        }
        if self.is_html("textarea") {
            return !self.has_attr("readonly") && !self.is_actually_disabled();
        }
        false
    }

    /// Whether the `required` attribute applies to this element, which
    /// is the set `:required` and `:optional` divide between them:
    /// `select`, `textarea`, and an `input` whose type state it applies
    /// to. An `input` outside that set matches neither pseudo-class,
    /// whatever attributes it carries.
    fn required_applies(&self) -> bool {
        if self.is_html("input") {
            return self.input_type().required_applies();
        }
        self.is_html("select") || self.is_html("textarea")
    }

    /// `:checked` — a checked checkbox or radio `input`, or a selected
    /// `option`. Checkedness and selectedness are runtime state; a
    /// static document has only the attributes that seed them, which is
    /// the crate's documented approximation and this reference's too.
    fn is_checked(&self) -> bool {
        if self.is_html("input") {
            return self.has_attr("checked")
                && matches!(self.input_type(), InputType::Checkbox | InputType::Radio);
        }
        self.is_html("option") && self.has_attr("selected")
    }

    /// A submit button: a `button` in the Submit state — `type` is an
    /// enumerated attribute whose missing and invalid value defaults
    /// are both Submit — or an `input` of type `submit` or `image`.
    fn is_submit_button(&self) -> bool {
        if self.is_html("button") {
            let ty = self.attr("type").unwrap_or_default().to_ascii_lowercase();
            return ty != "reset" && ty != "button";
        }
        self.is_html("input") && matches!(self.input_type(), InputType::Submit | InputType::Image)
    }

    /// The element's form owner, taken to be the nearest ancestor
    /// `form`. That is what it is for every control written inside its
    /// form; a control associated by a `form="id"` attribute instead is
    /// not followed, which is the crate's documented approximation and
    /// so has to be this reference's as well.
    fn form_owner(self) -> Option<El<'a>> {
        self.ancestors().find(|el| el.is_html("form"))
    }

    /// Whether this is its form owner's [default button][db]: the first
    /// submit button in tree order whose form owner is that form. The
    /// arena is built by a pre-order walk, so index order is tree
    /// order.
    ///
    /// [db]: https://html.spec.whatwg.org/multipage/forms.html#default-button
    fn is_default_button(self) -> bool {
        let Some(form) = self.form_owner() else {
            return false;
        };
        (0..self.dom.nodes.len())
            .map(|index| self.at(index))
            .find(|el| {
                el.is_submit_button()
                    && el
                        .form_owner()
                        .is_some_and(|owner| owner.index == form.index)
            })
            .is_some_and(|first| first.index == self.index)
    }

    /// `:default` — a checked checkbox or radio `input`, a selected
    /// `option`, or a form's default submit button.
    fn is_default(self) -> bool {
        self.is_checked() || (self.is_submit_button() && self.is_default_button())
    }

    /// `:placeholder-shown` — an `input` or `textarea` carrying a
    /// non-empty `placeholder` the type allows, whose value is empty.
    /// A document says only what the value *starts* as, so this is the
    /// state before any typing: the crate's documented approximation,
    /// restated here.
    fn is_placeholder_shown(&self) -> bool {
        let placeholder = self.attr("placeholder").unwrap_or_default();
        if placeholder.is_empty() {
            return false;
        }
        if self.is_html("input") {
            return self.input_type().placeholder_applies()
                && self.attr("value").unwrap_or_default().is_empty();
        }
        self.is_html("textarea") && self.node().text.is_empty()
    }

    /// `:link` and `:any-link` — an `a` or `area` with an `href`. The
    /// `link` element carries an `href` too but is not one of the two
    /// HTML names: "all `a` elements that have an `href` attribute, and
    /// all `area` elements that have an `href` attribute, must match one
    /// of :link and :visited".
    fn is_hyperlink(&self) -> bool {
        (self.is_html("a") || self.is_html("area")) && self.has_attr("href")
    }
}

impl Element for El<'_> {
    type Impl = RefImpl;

    fn opaque(&self) -> OpaqueElement {
        // The arena is never mutated after it is built, so a node's
        // address is a stable identity for as long as matching runs.
        OpaqueElement::new(self.node())
    }

    fn parent_element(&self) -> Option<Self> {
        self.node().parent.map(|index| self.at(index))
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.sibling(-1)
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.sibling(1)
    }

    fn first_element_child(&self) -> Option<Self> {
        self.node().children.first().map(|&index| self.at(index))
    }

    fn is_html_element_in_html_document(&self) -> bool {
        false
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.node().local == local_name
    }

    fn has_namespace(&self, ns: &str) -> bool {
        self.node().namespace == ns
    }

    fn is_same_type(&self, other: &Self) -> bool {
        let (this, other) = (self.node(), other.node());
        this.local == other.local && this.namespace == other.namespace
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&NsUrl>,
        local_name: &Name,
        operation: &AttrSelectorOperation<&Name>,
    ) -> bool {
        self.node()
            .attributes
            .iter()
            .filter(|attribute| {
                attribute.local == local_name.0
                    && match ns {
                        NamespaceConstraint::Any => true,
                        NamespaceConstraint::Specific(url) => attribute.namespace == url.0,
                    }
            })
            .any(|attribute| operation.eval_str(&attribute.value))
    }

    fn match_non_ts_pseudo_class(
        &self,
        pc: &PseudoClass,
        _context: &mut MatchingContext<RefImpl>,
    ) -> bool {
        match *pc {
            PseudoClass::Disabled => self.is_disableable() && self.is_actually_disabled(),
            PseudoClass::Enabled => self.is_disableable() && !self.is_actually_disabled(),
            PseudoClass::Checked => self.is_checked(),
            PseudoClass::Required => self.required_applies() && self.has_attr("required"),
            PseudoClass::Optional => self.required_applies() && !self.has_attr("required"),
            PseudoClass::ReadWrite => self.is_read_write(),
            PseudoClass::ReadOnly => !self.is_read_write(),
            PseudoClass::Default => self.is_default(),
            PseudoClass::PlaceholderShown => self.is_placeholder_shown(),
            PseudoClass::Link => self.is_hyperlink(),
        }
    }

    fn match_pseudo_element(&self, _pe: &Never, _context: &mut MatchingContext<RefImpl>) -> bool {
        Never::unreachable()
    }

    fn apply_selector_flags(&self, _flags: ElementSelectorFlags) {
        // Matching runs with NeedsSelectorFlags::No.
    }

    fn is_link(&self) -> bool {
        false
    }

    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &Name, case_sensitivity: CaseSensitivity) -> bool {
        self.node()
            .id
            .as_ref()
            .is_some_and(|own| case_sensitivity.eq(own.as_bytes(), id.0.as_bytes()))
    }

    fn has_class(&self, name: &Name, case_sensitivity: CaseSensitivity) -> bool {
        self.node()
            .classes
            .iter()
            .any(|own| case_sensitivity.eq(own.as_bytes(), name.0.as_bytes()))
    }

    fn has_custom_state(&self, _name: &Name) -> bool {
        false
    }

    fn imported_part(&self, _name: &Name) -> Option<Name> {
        None
    }

    fn is_part(&self, _name: &Name) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        self.node().empty
    }

    fn is_root(&self) -> bool {
        self.node().parent.is_none()
    }

    fn add_element_unique_hashes(&self, _filter: &mut BloomFilter) -> bool {
        false
    }
}

/// A document indexed for matching, plus the parser that resolves
/// its namespace prefixes.
pub(crate) struct Reference {
    dom: Dom,
    parser: RefParser,
}

impl Reference {
    /// A reference whose grammar is the tree-structural selectors
    /// alone: every pseudo-class whose answer is not in the tree is
    /// rejected at parse time.
    pub(crate) fn new(document: Document<'_>, namespaces: &[(&str, &str)]) -> Self {
        Self::build(document, namespaces, /* html = */ false)
    }

    /// A reference that also answers HTML's static pseudo-classes (see
    /// [`PseudoClass`]), for the HTML-mode differential suite.
    pub(crate) fn new_html(document: Document<'_>, namespaces: &[(&str, &str)]) -> Self {
        Self::build(document, namespaces, /* html = */ true)
    }

    fn build(document: Document<'_>, namespaces: &[(&str, &str)], html: bool) -> Self {
        Reference {
            dom: Dom::build(document),
            parser: RefParser {
                namespaces: namespaces
                    .iter()
                    .map(|(prefix, url)| ((*prefix).to_owned(), (*url).to_owned()))
                    .collect(),
                html,
            },
        }
    }

    /// The labels of the elements `css` matches, in document order,
    /// or the parse error if the reference parser rejects it.
    pub(crate) fn select(&self, css: &str) -> Result<Vec<String>, String> {
        let mut input = ParserInput::new(css);
        let mut parser = CssParser::new(&mut input);
        let list = SelectorList::parse(&self.parser, &mut parser, ParseRelative::No)
            .map_err(|e| format!("{css:?}: {e:?}"))?;

        let mut selected = Vec::new();
        for index in 0..self.dom.nodes.len() {
            let element = El {
                dom: &self.dom,
                index,
            };
            // A fresh context per element: the caches are an
            // optimisation this reference has no need of, and a
            // reference that shares no state between elements is
            // one less thing to trust.
            let mut caches = SelectorCaches::default();
            let mut context = MatchingContext::new(
                MatchingMode::Normal,
                None,
                &mut caches,
                QuirksMode::NoQuirks,
                NeedsSelectorFlags::No,
                MatchingForInvalidation::No,
            );
            if list
                .slice()
                .iter()
                .any(|selector| matches_selector(selector, 0, None, &element, &mut context))
            {
                selected.push(self.dom.nodes[index].label.clone());
            }
        }
        Ok(selected)
    }
}
