//! Shared helpers for the integration tests: the selector corpus, the
//! sxd-xpath evaluation harness, and the fixture documents.
//!
//! `tests/` binaries each pull this in with `mod common;`, so any single
//! binary uses only part of it; hence the blanket `dead_code` allow.
#![allow(dead_code)]

use css_to_xpath::Mode;
use sxd_document::Package;
use sxd_xpath::nodeset::Node;
use sxd_xpath::{Context, Factory, Value, context, function};

/// The implicit XML namespace, needed to resolve `@xml:lang` name tests.
pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
pub const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";
pub const DC_NS: &str = "http://purl.org/dc/elements/1.1/";

pub const MODES: [Mode; 3] = [Mode::Generic, Mode::Html, Mode::Xhtml];

pub fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Generic => "generic",
        Mode::Html => "html",
        Mode::Xhtml => "xhtml",
    }
}

/// The shared selector corpus: every CSS selector literal that the unit
/// tests in `src/lib.rs` translate, plus the README's examples.
///
/// One selector per line, stored raw — so a selector containing a line
/// break cannot be represented here. That is the only gap, and the fuzz
/// target covers arbitrary bytes anyway.
pub fn corpus() -> impl Iterator<Item = &'static str> {
    include_str!("../corpus/selectors.txt").lines()
}

/// A parsed fixture document plus the prefix bindings an evaluation of
/// this crate's output needs.
///
/// The translator identity-maps CSS namespace prefixes onto XPath ones
/// (`svg|rect` becomes `svg:rect`) and never resolves them to a URI, so
/// the *evaluator* is where a prefix acquires its meaning. Every prefix
/// a fixture uses is bound here to the same URI the document declares.
pub struct Fixture {
    package: Package,
    namespaces: Vec<(&'static str, &'static str)>,
}

impl Fixture {
    pub fn new(xml: &str, namespaces: &[(&'static str, &'static str)]) -> Self {
        let package = sxd_document::parser::parse(xml)
            .unwrap_or_else(|e| panic!("fixture is not well-formed XML: {e}"));
        Fixture {
            package,
            namespaces: namespaces.to_vec(),
        }
    }

    /// Translate `css` in `mode` and return the `id` attributes of the
    /// elements the resulting XPath selects, in document order.
    ///
    /// The `descendant-or-self::` prefix is the README's "search the
    /// whole document" form; evaluation starts at the root node.
    pub fn select(&self, css: &str, mode: Mode) -> Vec<String> {
        self.select_from(css, mode, None)
    }

    /// As [`Fixture::select`], but evaluated with the element carrying
    /// `scope_id` as the context node — what `:scope` refers to.
    pub fn select_scoped(&self, css: &str, mode: Mode, scope_id: &str) -> Vec<String> {
        self.select_from(css, mode, Some(scope_id))
    }

    fn select_from(&self, css: &str, mode: Mode, scope_id: Option<&str>) -> Vec<String> {
        let xpath_src = css_to_xpath::css_to_xpath(css, "descendant-or-self::", mode)
            .unwrap_or_else(|e| {
                panic!("{} mode failed to translate {css:?}: {e}", mode_name(mode))
            });
        self.evaluate(&xpath_src, scope_id)
            .unwrap_or_else(|e| panic!("evaluating {xpath_src:?} (from {css:?}): {e}"))
    }

    /// Evaluate a raw XPath expression, returning node labels in
    /// document order, or an error string.
    pub fn evaluate(&self, xpath_src: &str, scope_id: Option<&str>) -> Result<Vec<String>, String> {
        let document = self.package.as_document();

        let xpath = Factory::new()
            .build(xpath_src)
            .map_err(|e| format!("XPath parse error: {e}"))?
            .ok_or_else(|| "empty XPath expression".to_string())?;

        let mut context = Context::new();
        context.set_function("lang", Lang);
        context.set_namespace("xml", XML_NS);
        for (prefix, uri) in &self.namespaces {
            context.set_namespace(prefix, uri);
        }

        let start: Node = match scope_id {
            None => document.root().into(),
            Some(id) => find_by_id(document.root().into(), id)
                .ok_or_else(|| format!("no element with id {id:?} in the fixture"))?,
        };

        match xpath
            .evaluate(&context, start)
            .map_err(|e| format!("XPath evaluation error: {e}"))?
        {
            Value::Nodeset(nodes) => Ok(nodes.document_order().into_iter().map(label).collect()),
            other => Err(format!("expected a nodeset, got {other:?}")),
        }
    }
}

/// How a selected node is reported. Elements are named by their `id`,
/// which every element in the fixtures carries; anything else is
/// labelled so an unexpected match is legible rather than silent.
fn label(node: Node<'_>) -> String {
    match node {
        Node::Element(el) => el.attribute_value("id").map_or_else(
            || format!("<{} with no id>", el.name().local_part()),
            ToString::to_string,
        ),
        Node::Attribute(at) => format!("@{}", at.name().local_part()),
        Node::Root(_) => "/".to_string(),
        Node::Text(_) => "#text".to_string(),
        Node::Comment(_) => "#comment".to_string(),
        Node::ProcessingInstruction(_) => "#pi".to_string(),
        Node::Namespace(ns) => format!("xmlns:{}", ns.prefix()),
    }
}

fn find_by_id<'d>(node: Node<'d>, id: &str) -> Option<Node<'d>> {
    if let Node::Element(el) = node
        && el.attribute_value("id") == Some(id)
    {
        return Some(node);
    }
    node.children().into_iter().find_map(|c| find_by_id(c, id))
}

/// XPath 1.0's `lang()`, which sxd-xpath's core function library omits.
///
/// `Mode::Generic` translates `:lang()` to it, so without this the
/// generic `:lang` cases could not be evaluated at all. Implemented per
/// XPath 1.0 §4.3: the nearest ancestor-or-self `xml:lang`, compared
/// case-insensitively, matching either exactly or up to a `-` suffix.
struct Lang;

impl function::Function for Lang {
    fn evaluate<'c, 'd>(
        &self,
        context: &context::Evaluation<'c, 'd>,
        args: Vec<Value<'d>>,
    ) -> Result<Value<'d>, function::Error> {
        let wanted = args
            .first()
            .ok_or(function::Error::ArgumentMissing)?
            .string()
            .to_lowercase();

        let mut node = Some(context.node);
        while let Some(current) = node {
            if let Node::Element(el) = current
                && let Some(tag) = el.attribute_value((XML_NS, "lang"))
            {
                let tag = tag.to_lowercase();
                let matches = tag == wanted
                    || (tag.len() > wanted.len()
                        && tag.starts_with(&wanted)
                        && tag.as_bytes()[wanted.len()] == b'-');
                return Ok(Value::Boolean(matches));
            }
            node = current.parent();
        }
        Ok(Value::Boolean(false))
    }
}
