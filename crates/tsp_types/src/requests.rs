//! TSP request and response types.
//!
//! This module defines the parameter and result types for all TSP requests
//! and notifications.
//!
//! These types match the TypeScript protocol defined in `typeServerProtocol.ts`.

use lsp_types::{Range, Url};
use serde::{Deserialize, Serialize};

use crate::types::Declaration;

/// A node in the AST, identified by URI and range.
/// Matches `TypeServerProtocol.Node` in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// The URI of the document containing the node.
    pub uri: Url,
    /// The range of the node in the document.
    pub range: Range,
}

/// Represents the `Declaration | Node` union used as the `arg` parameter
/// in type query requests (getComputedType, getExpectedType, getDeclaredType).
///
/// When the caller has a `Declaration` (with `kind`, `category`, `node` fields),
/// it sends the full Declaration object. When the caller only has a position,
/// it sends a plain `Node` (with just `uri` and `range`).
///
/// Deserialization checks for the presence of a `kind` field to discriminate:
/// - `kind` present → Declaration (Regular or Synthesized)
/// - `kind` absent → plain Node
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationOrNode {
    Declaration(Declaration),
    Node(Node),
}

impl DeclarationOrNode {
    /// Extract the `Node` from this value.
    ///
    /// For a `Declaration::Regular`, returns the inner `node` field.
    /// For a plain `Node`, returns it directly.
    /// For a `Declaration::Synthesized`, returns `None` (no source node).
    pub fn node(&self) -> Option<&Node> {
        match self {
            DeclarationOrNode::Node(n) => Some(n),
            DeclarationOrNode::Declaration(Declaration::Regular { node, .. }) => Some(node),
            DeclarationOrNode::Declaration(Declaration::Synthesized { .. }) => None,
        }
    }
}

impl Serialize for DeclarationOrNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            DeclarationOrNode::Declaration(d) => d.serialize(serializer),
            DeclarationOrNode::Node(n) => n.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for DeclarationOrNode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;

        // If a "kind" field is present, this is a Declaration; otherwise it's a Node.
        if value.get("kind").is_some() {
            let decl: Declaration =
                serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(DeclarationOrNode::Declaration(decl))
        } else {
            let node: Node = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
            Ok(DeclarationOrNode::Node(node))
        }
    }
}

/// A module name descriptor for import resolution.
/// Matches `TypeServerProtocol.ModuleName` in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDescriptor {
    /// The leading dots for relative imports (0 for absolute imports).
    pub leading_dots: u32,
    /// The parts of the module name split by dots.
    /// For example, `os.path` would be `["os", "path"]`.
    pub name_parts: Vec<String>,
}

impl ModuleDescriptor {
    /// Convert to a dotted module name string.
    pub fn to_module_name(&self) -> String {
        let dots = ".".repeat(self.leading_dots as usize);
        let name = self.name_parts.join(".");
        format!("{dots}{name}")
    }
}

// ============================================================================
// Protocol Version
// ============================================================================

// Note: `typeServer/getSupportedProtocolVersion` takes no params and returns
// a raw `string` (the version in semver format like "0.4.0").

// ============================================================================
// Snapshot
// ============================================================================

// Note: `typeServer/getSnapshot` takes no params and returns a raw `number`.

/// Parameters for `typeServer/snapshotChanged` notification.
/// Matches `TypeServerProtocol.SnapshotChangedNotification` in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotChangedParams {
    /// The old snapshot number.
    pub old: u64,
    /// The new snapshot number.
    pub new: u64,
}

// ============================================================================
// Search Paths
// ============================================================================

/// Parameters for `typeServer/getPythonSearchPaths`.
/// Matches `TypeServerProtocol.GetPythonSearchPathsParams` in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPythonSearchPathsParams {
    /// Root folder to get search paths from.
    pub from_uri: String,
    /// The snapshot to use for the query.
    pub snapshot: u64,
}

// Note: `typeServer/getPythonSearchPaths` returns `string[] | undefined` directly,
// not a wrapped object.

// ============================================================================
// Import Resolution
// ============================================================================

/// Parameters for `typeServer/resolveImport`.
/// Matches `TypeServerProtocol.ResolveImportParams` in TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveImportParams {
    /// The URI of the source file where the import is referenced.
    pub source_uri: String,
    /// The descriptor of the imported module.
    pub module_descriptor: ModuleDescriptor,
    /// The snapshot to use for the query.
    pub snapshot: u64,
}

// Note: `typeServer/resolveImport` returns `string | undefined` directly
// (the resolved URI or null).

// ============================================================================
// Type Queries
// ============================================================================

/// Parameters for `typeServer/getComputedType`.
/// Matches the TypeScript protocol: `{ arg: Declaration | Node; snapshot: number }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetComputedTypeParams {
    /// The declaration or node to get the type for.
    pub arg: DeclarationOrNode,
    /// The snapshot to use for the query.
    pub snapshot: u64,
}

// Note: `typeServer/getComputedType` returns `Type | undefined` directly.

/// Parameters for `typeServer/getExpectedType`.
/// Matches the TypeScript protocol: `{ arg: Declaration | Node; snapshot: number }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetExpectedTypeParams {
    /// The declaration or node to get the expected type for.
    pub arg: DeclarationOrNode,
    /// The snapshot to use for the query.
    pub snapshot: u64,
}

// Note: `typeServer/getExpectedType` returns `Type | undefined` directly.

/// Parameters for `typeServer/getDeclaredType`.
/// Matches the TypeScript protocol: `{ arg: Declaration | Node; snapshot: number }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetDeclaredTypeParams {
    /// The declaration or node to get the declared type for.
    pub arg: DeclarationOrNode,
    /// The snapshot to use for the query.
    pub snapshot: u64,
}

// Note: `typeServer/getDeclaredType` returns `Type | undefined` directly.

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;

    #[test]
    fn test_node_serialization() {
        let node = Node {
            uri: Url::parse("file:///test.py").unwrap(),
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
        };

        let json = serde_json::to_string(&node).unwrap();
        let parsed: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(node, parsed);
    }

    #[test]
    fn test_snapshot_changed_params_serialization() {
        let params = SnapshotChangedParams { old: 1, new: 2 };
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(json, r#"{"old":1,"new":2}"#);
    }

    #[test]
    fn test_module_descriptor_serialization() {
        // Absolute import: os.path
        let desc = ModuleDescriptor {
            leading_dots: 0,
            name_parts: vec!["os".to_string(), "path".to_string()],
        };
        let json = serde_json::to_string(&desc).unwrap();
        assert_eq!(json, r#"{"leadingDots":0,"nameParts":["os","path"]}"#);

        // Relative import: from . import foo
        let desc2 = ModuleDescriptor {
            leading_dots: 1,
            name_parts: vec!["foo".to_string()],
        };
        let json2 = serde_json::to_string(&desc2).unwrap();
        assert_eq!(json2, r#"{"leadingDots":1,"nameParts":["foo"]}"#);
    }

    #[test]
    fn test_module_descriptor_to_module_name() {
        let desc = ModuleDescriptor {
            leading_dots: 0,
            name_parts: vec!["os".to_string(), "path".to_string()],
        };
        assert_eq!(desc.to_module_name(), "os.path");

        let desc2 = ModuleDescriptor {
            leading_dots: 2,
            name_parts: vec!["foo".to_string()],
        };
        assert_eq!(desc2.to_module_name(), "..foo");
    }

    #[test]
    fn test_get_python_search_paths_params_serialization() {
        let params = GetPythonSearchPathsParams {
            from_uri: "file:///workspace".to_string(),
            snapshot: 42,
        };
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(json, r#"{"fromUri":"file:///workspace","snapshot":42}"#);
    }

    #[test]
    fn test_resolve_import_params_serialization() {
        let params = ResolveImportParams {
            source_uri: "file:///test.py".to_string(),
            module_descriptor: ModuleDescriptor {
                leading_dots: 0,
                name_parts: vec!["os".to_string()],
            },
            snapshot: 1,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: ResolveImportParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, parsed);
    }

    #[test]
    fn test_get_computed_type_params_with_node() {
        let params = GetComputedTypeParams {
            arg: DeclarationOrNode::Node(Node {
                uri: Url::parse("file:///test.py").unwrap(),
                range: Range {
                    start: Position { line: 0, character: 0 },
                    end: Position { line: 0, character: 5 },
                },
            }),
            snapshot: 1,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: GetComputedTypeParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, parsed);
    }

    #[test]
    fn test_get_computed_type_params_with_declaration() {
        // Simulate what Pylance sends when passing a Declaration as arg
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
        // Should be a Declaration, and we can extract the inner Node
        let node = parsed.arg.node().expect("Regular declaration should have a node");
        assert_eq!(node.uri.as_str(), "file:///test.py");
        assert_eq!(node.range.start.line, 0);
    }

    #[test]
    fn test_declaration_or_node_plain_node() {
        // Plain Node has no "kind" field
        let json = r#"{"uri": "file:///test.py", "range": {"start": {"line": 1, "character": 2}, "end": {"line": 1, "character": 5}}}"#;
        let parsed: DeclarationOrNode = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, DeclarationOrNode::Node(_)));
        assert!(parsed.node().is_some());
    }

    #[test]
    fn test_declaration_or_node_regular_declaration() {
        // Regular Declaration has "kind": 0
        let json = r#"{"kind": 0, "category": 5, "name": "my_func", "node": {"uri": "file:///test.py", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 7}}}}"#;
        let parsed: DeclarationOrNode = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, DeclarationOrNode::Declaration(Declaration::Regular { .. })));
        let node = parsed.node().expect("Regular declaration should have a node");
        assert_eq!(node.uri.as_str(), "file:///test.py");
    }

    #[test]
    fn test_declaration_or_node_synthesized_declaration() {
        // Synthesized Declaration has "kind": 1
        let json = r#"{"kind": 1, "uri": "file:///builtins.pyi"}"#;
        let parsed: DeclarationOrNode = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, DeclarationOrNode::Declaration(Declaration::Synthesized { .. })));
        // Synthesized declarations have no source node
        assert!(parsed.node().is_none());
    }
}




