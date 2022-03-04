use async_trait::async_trait;
use eyre::Result;
use node_address::Address;
use node_pointer::{walk, Visitor};
use serde::{Deserialize, Serialize};
use std::path::Path;
use stencila_schema::{InlineContent, Node};

// Export and re-export for the convenience of crates that implement a provider
pub use ::async_trait;
pub use ::codecs;
pub use ::eyre;
pub use ::http_utils;
pub use ::once_cell;
pub use ::regex;
pub use ::stencila_schema;
pub use ::tracing;

/// A specification for providers
///
/// All providers, including those implemented in plugins, should provide this
/// specification. Rust implementations return a `Provider` instance from the
/// `spec` function of `ProviderTrait`. Plugins provide a JSON or YAML serialization
/// as part of their manifest.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Provider {
    /// The name of the provider
    pub name: String,
}

impl Provider {
    pub fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: name.as_ref().to_string(),
        }
    }
}

/// A trait for providers
///
/// This trait can be used by Rust implementations of providers, allowing them to
/// be compiled into the Stencila binaries.
#[async_trait]
pub trait ProviderTrait {
    /// Get the [`Provider`] specification
    fn spec() -> Provider;

    /// Parse a string into a node
    fn parse(_string: &str) -> Vec<ParseItem> {
        Vec::new()
    }

    /// Detect nodes within a root node that the provider may be able to identify and enrich.
    ///
    /// Returns a vector of [`Detection`].
    async fn detect(root: &Node) -> Result<Vec<DetectItem>> {
        let name = Self::spec().name;
        let parse = Box::new(|string: &str| Self::parse(string));

        let mut detector = Detector::new(name, parse);
        walk(root, &mut detector);
        Ok(detector.detections)
    }

    /// Identify a node
    ///
    /// The node is supplied to the provider, with one or more properties populated.
    /// The provider then attempts to identify the node based on those properties,
    /// and if it was able to do so, returns a copy of the node with one or more identifying
    /// properties populated (e.g. the `GithubProvider` might populate the `codeRepository` property
    /// of a `SofwareSourceCode` node).
    async fn identify(node: Node) -> Result<Node> {
        Ok(node)
    }

    /// Enrich a node
    ///
    /// If the provider had previously identified the node, then the relevant identifiers
    /// will be used to fetch enrichment data, otherwise `identify` will be called.
    /// Then, the provider will return a opy of the node with properties that are missing.
    async fn enrich(node: Node, _options: Option<EnrichOptions>) -> Result<Node> {
        Ok(node)
    }

    /// Import files associated with a resource, from the provider, into a project
    async fn import(_node: &Node, _dest: &Path, _options: Option<ImportOptions>) -> Result<bool> {
        Ok(false)
    }

    /// Watch a resource and import files associated with it they change
    async fn watch(_node: &Node, _dest: &Path, _options: Option<WatchOptions>) -> Result<bool> {
        Ok(false)
    }
}

#[derive(Debug, Default, Clone)]
pub struct EnrichOptions {
    pub token: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ImportOptions {
    pub token: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct WatchOptions {
    pub token: Option<String>,

    /// The URL to listen on
    pub url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParseItem {
    /// The start position in the string that the node was parsed from
    pub begin: usize,

    /// The end position in the string that the node was parsed from
    pub end: usize,

    /// The parsed [`Node`] usually with some properties populated
    pub node: Node,
}

#[derive(Debug, Serialize)]
pub struct DetectItem {
    /// The name of the provider that detected the node
    pub provider: String,

    /// The percent confidence in the detection (0-100)
    pub confidence: u32,

    /// The [`Address`], within the node tree, that the node detected node begins
    pub begin: Address,

    /// The [`Address`], within the node tree, that the node detected node ends
    pub end: Address,

    /// The detected [`Node`] usually with some properties populated (i.e. those
    /// properties that were used to detect it)
    pub node: Node,
}

pub struct Detector {
    /// The name of the provider that this detector is for
    provider: String,

    /// The function used to attempt to parse a string into a node
    parse: Box<dyn Fn(&str) -> Vec<ParseItem>>,

    /// The list of detected nodes and their location
    detections: Vec<DetectItem>,
}

impl Detector {
    fn new(provider: String, parse: Box<dyn Fn(&str) -> Vec<ParseItem>>) -> Self {
        Self {
            provider,
            parse,
            detections: Vec::new(),
        }
    }

    fn visit_string(&mut self, address: &Address, string: &str) {
        let nodes = (self.parse)(string);
        let mut detections = nodes
            .into_iter()
            .map(|ParseItem { begin, end, node }| DetectItem {
                provider: self.provider.clone(),
                confidence: 100,
                begin: address.add_index(begin),
                end: address.add_index(end),
                node,
            })
            .collect();
        self.detections.append(&mut detections);
    }
}

impl Visitor for Detector {
    fn visit_node(&mut self, address: &Address, node: &Node) -> bool {
        if let Node::String(string) = node {
            self.visit_string(address, string);
            false
        } else {
            true
        }
    }

    fn visit_inline(&mut self, address: &Address, node: &InlineContent) -> bool {
        if let InlineContent::String(string) = node {
            self.visit_string(address, string);
            false
        } else {
            true
        }
    }
}
