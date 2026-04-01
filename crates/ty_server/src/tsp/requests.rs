//! TSP request and response types.
//!
//! Parameter and result types for all TSP requests and notifications,
//! plus custom `lsp_types::request::Request` impls for dispatch integration.

use lsp_types::{Range, Url};
use serde::{Deserialize, Serialize};

use crate::tsp::protocol::methods;
use crate::tsp::types::{Declaration, Type};

// -- Shared types ------------------------------------------------------------

/// An AST node identified by document URI and range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Node {
    pub uri: Url,
    pub range: Range,
}

/// The `arg` parameter for type query requests — either a full `Declaration`
/// or a plain `Node`.
///
/// Discriminated by presence of the `kind` field:
/// - `kind` present → `Declaration`
/// - `kind` absent  → plain `Node`
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DeclarationOrNode {
    Declaration(Declaration),
    Node(Node),
}

impl DeclarationOrNode {
    /// Extract the inner `Node`, if available.
    ///
    /// Returns `None` for `Declaration::Synthesized` (no source node).
    pub(crate) fn node(&self) -> Option<&Node> {
        match self {
            Self::Node(n) => Some(n),
            Self::Declaration(Declaration::Regular { node, .. }) => Some(node),
            Self::Declaration(Declaration::Synthesized { .. }) => None,
        }
    }
}

impl Serialize for DeclarationOrNode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Declaration(d) => d.serialize(serializer),
            Self::Node(n) => n.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for DeclarationOrNode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value.get("kind").is_some() {
            serde_json::from_value(value)
                .map(Self::Declaration)
                .map_err(serde::de::Error::custom)
        } else {
            serde_json::from_value(value)
                .map(Self::Node)
                .map_err(serde::de::Error::custom)
        }
    }
}

/// A module name descriptor for import resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModuleDescriptor {
    /// Leading dots for relative imports (0 for absolute).
    pub leading_dots: u32,
    /// Module name parts split by dots (e.g., `["os", "path"]`).
    pub name_parts: Vec<String>,
}

impl ModuleDescriptor {
    /// Convert to a dotted module name string.
    pub(crate) fn to_module_name(&self) -> String {
        let dots = ".".repeat(self.leading_dots as usize);
        let name = self.name_parts.join(".");
        format!("{dots}{name}")
    }
}

// -- Snapshot ----------------------------------------------------------------

/// Parameters for `typeServer/snapshotChanged` notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotChangedParams {
    pub old: u64,
    pub new: u64,
}

// -- Search paths ------------------------------------------------------------

/// Parameters for `typeServer/getPythonSearchPaths`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetPythonSearchPathsParams {
    pub from_uri: String,
    pub snapshot: u64,
}

// -- Import resolution -------------------------------------------------------

/// Parameters for `typeServer/resolveImport`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveImportParams {
    pub source_uri: String,
    pub module_descriptor: ModuleDescriptor,
    pub snapshot: u64,
}

// -- Type queries ------------------------------------------------------------

/// Parameters for `typeServer/getComputedType`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetComputedTypeParams {
    pub arg: DeclarationOrNode,
    pub snapshot: u64,
}

/// Parameters for `typeServer/getExpectedType`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetExpectedTypeParams {
    pub arg: DeclarationOrNode,
    pub snapshot: u64,
}

/// Parameters for `typeServer/getDeclaredType`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GetDeclaredTypeParams {
    pub arg: DeclarationOrNode,
    pub snapshot: u64,
}

// -- Custom LSP request types for dispatch -----------------------------------

/// `typeServer/getSupportedProtocolVersion` — returns a version string.
pub(crate) enum GetSupportedProtocolVersion {}

impl lsp_types::request::Request for GetSupportedProtocolVersion {
    type Params = ();
    type Result = String;
    const METHOD: &'static str = methods::GET_SUPPORTED_PROTOCOL_VERSION;
}

/// `typeServer/getSnapshot` — returns the current snapshot number.
pub(crate) enum GetSnapshot {}

impl lsp_types::request::Request for GetSnapshot {
    type Params = ();
    type Result = u64;
    const METHOD: &'static str = methods::GET_SNAPSHOT;
}

/// `typeServer/getPythonSearchPaths` — returns search paths for a workspace.
pub(crate) enum GetPythonSearchPaths {}

impl lsp_types::request::Request for GetPythonSearchPaths {
    type Params = GetPythonSearchPathsParams;
    type Result = Option<Vec<String>>;
    const METHOD: &'static str = methods::GET_PYTHON_SEARCH_PATHS;
}

/// `typeServer/resolveImport` — resolves a module name to a file URI.
pub(crate) enum ResolveImport {}

impl lsp_types::request::Request for ResolveImport {
    type Params = ResolveImportParams;
    type Result = Option<String>;
    const METHOD: &'static str = methods::RESOLVE_IMPORT;
}

/// `typeServer/getComputedType` — returns the inferred type at a position.
pub(crate) enum GetComputedType {}

impl lsp_types::request::Request for GetComputedType {
    type Params = GetComputedTypeParams;
    type Result = Option<Type>;
    const METHOD: &'static str = methods::GET_COMPUTED_TYPE;
}

/// `typeServer/getExpectedType` — returns the expected type at a position.
pub(crate) enum GetExpectedType {}

impl lsp_types::request::Request for GetExpectedType {
    type Params = GetExpectedTypeParams;
    type Result = Option<Type>;
    const METHOD: &'static str = methods::GET_EXPECTED_TYPE;
}

/// `typeServer/getDeclaredType` — returns the declared type at a position.
pub(crate) enum GetDeclaredType {}

impl lsp_types::request::Request for GetDeclaredType {
    type Params = GetDeclaredTypeParams;
    type Result = Option<Type>;
    const METHOD: &'static str = methods::GET_DECLARED_TYPE;
}

// -- Custom LSP notification types -------------------------------------------

/// `typeServer/snapshotChanged` — sent to the client when the snapshot changes.
pub(crate) enum SnapshotChanged {}

impl lsp_types::notification::Notification for SnapshotChanged {
    type Params = SnapshotChangedParams;
    const METHOD: &'static str = methods::SNAPSHOT_CHANGED;
}

// -- Tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    #[test]
    fn node_roundtrip() {
        let node = Node {
            uri: Url::parse("file:///test.py").unwrap(),
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(0, 5),
            },
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(node, back);
    }

    #[test]
    fn declaration_or_node_plain_node() {
        let json = r#"{"uri": "file:///test.py", "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 5}}}"#;
        let parsed: DeclarationOrNode = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, DeclarationOrNode::Node(_)));
        assert!(parsed.node().is_some());
    }

    #[test]
    fn declaration_or_node_regular_declaration() {
        let json = r#"{"kind": 0, "category": 5, "name": "my_func", "node": {"uri": "file:///test.py", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 7}}}}"#;
        let parsed: DeclarationOrNode = serde_json::from_str(json).unwrap();
        assert!(matches!(
            parsed,
            DeclarationOrNode::Declaration(Declaration::Regular { .. })
        ));
        let node = parsed
            .node()
            .expect("regular declaration should have a node");
        assert_eq!(node.uri.as_str(), "file:///test.py");
    }

    #[test]
    fn declaration_or_node_synthesized() {
        let json = r#"{"kind": 1, "uri": "file:///builtins.pyi"}"#;
        let parsed: DeclarationOrNode = serde_json::from_str(json).unwrap();
        assert!(matches!(
            parsed,
            DeclarationOrNode::Declaration(Declaration::Synthesized { .. })
        ));
        assert!(parsed.node().is_none());
    }

    #[test]
    fn module_descriptor_absolute() {
        let desc = ModuleDescriptor {
            leading_dots: 0,
            name_parts: vec!["os".to_string(), "path".to_string()],
        };
        assert_eq!(desc.to_module_name(), "os.path");
        let json = serde_json::to_string(&desc).unwrap();
        assert_eq!(json, r#"{"leadingDots":0,"nameParts":["os","path"]}"#);
    }

    #[test]
    fn module_descriptor_relative() {
        let desc = ModuleDescriptor {
            leading_dots: 2,
            name_parts: vec!["foo".to_string()],
        };
        assert_eq!(desc.to_module_name(), "..foo");
    }

    #[test]
    fn snapshot_changed_params_serialization() {
        let params = SnapshotChangedParams { old: 1, new: 2 };
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(json, r#"{"old":1,"new":2}"#);
    }

    #[test]
    fn search_paths_params_camel_case() {
        let params = GetPythonSearchPathsParams {
            from_uri: "file:///workspace".to_string(),
            snapshot: 42,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["fromUri"], "file:///workspace");
        assert_eq!(json["snapshot"], 42);
        assert!(json.get("from_uri").is_none());
    }

    #[test]
    fn resolve_import_params_roundtrip() {
        let params = ResolveImportParams {
            source_uri: "file:///test.py".to_string(),
            module_descriptor: ModuleDescriptor {
                leading_dots: 0,
                name_parts: vec!["os".to_string()],
            },
            snapshot: 1,
        };
        let json = serde_json::to_string(&params).unwrap();
        let back: ResolveImportParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, back);
    }

    #[test]
    fn get_computed_type_params_with_node() {
        let params = GetComputedTypeParams {
            arg: DeclarationOrNode::Node(Node {
                uri: Url::parse("file:///test.py").unwrap(),
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(0, 5),
                },
            }),
            snapshot: 1,
        };
        let json = serde_json::to_string(&params).unwrap();
        let back: GetComputedTypeParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, back);
    }

    #[test]
    fn get_computed_type_params_with_declaration() {
        let json = r#"{
            "arg": {
                "kind": 0,
                "category": 6,
                "name": "Literal",
                "node": {
                    "uri": "file:///test.py",
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 5}
                    }
                }
            },
            "snapshot": 1
        }"#;
        let parsed: GetComputedTypeParams = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.snapshot, 1);
        let node = parsed
            .arg
            .node()
            .expect("regular declaration should have a node");
        assert_eq!(node.uri.as_str(), "file:///test.py");
    }

    #[test]
    fn request_method_constants_match() {
        assert_eq!(
            <GetSupportedProtocolVersion as lsp_types::request::Request>::METHOD,
            "typeServer/getSupportedProtocolVersion"
        );
        assert_eq!(
            <GetSnapshot as lsp_types::request::Request>::METHOD,
            "typeServer/getSnapshot"
        );
        assert_eq!(
            <GetComputedType as lsp_types::request::Request>::METHOD,
            "typeServer/getComputedType"
        );
        assert_eq!(
            <GetExpectedType as lsp_types::request::Request>::METHOD,
            "typeServer/getExpectedType"
        );
        assert_eq!(
            <GetDeclaredType as lsp_types::request::Request>::METHOD,
            "typeServer/getDeclaredType"
        );
        assert_eq!(
            <GetPythonSearchPaths as lsp_types::request::Request>::METHOD,
            "typeServer/getPythonSearchPaths"
        );
        assert_eq!(
            <ResolveImport as lsp_types::request::Request>::METHOD,
            "typeServer/resolveImport"
        );
    }
}
