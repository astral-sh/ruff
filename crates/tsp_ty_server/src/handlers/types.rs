//! Handlers for type-related TSP requests.
//!
//! - `typeServer/getComputedType`
//! - `typeServer/getExpectedType`
//! - `typeServer/getDeclaredType`
//!
//! These handlers use `ty_ide`'s public `type_info` API to get type information
//! at specific positions.

use lsp_types::Url;
use ruff_db::files::system_path_to_file;
use ruff_db::source::{line_index, source_text};
use ruff_db::system::SystemPathBuf;
use ruff_source_file::PositionEncoding as RuffPositionEncoding;
use ruff_text_size::TextSize;
use tsp_types::types::{Declaration, DeclarationCategory, Type, TypeId};
use tsp_types::{GetComputedTypeParams, GetDeclaredTypeParams, GetExpectedTypeParams};
use ty_ide::{
    DeclarationInfo, TypeCategory as TyCategory, TypeInfo, declared_type_info, expected_type_info,
    type_info,
};
use ty_project::ProjectDatabase;

use crate::type_serializer::{ResolvedDeclaration, convert_type_info};
use crate::{SnapshotManager, typeshed_cache};

/// Handle the `typeServer/getComputedType` request.
///
/// Returns the computed type for an expression after flow-sensitive narrowing.
/// Uses `ty_ide::type_info` to get structured type information at the specified position.
///
/// Per the TSP protocol, returns `Type | undefined` directly.
pub(crate) fn handle_get_computed_type(
    params: &GetComputedTypeParams,
    snapshot_manager: &SnapshotManager,
    databases: &[ProjectDatabase],
) -> Result<serde_json::Value, String> {
    // Validate the snapshot
    let current = snapshot_manager.current();
    if params.snapshot != current {
        return Err(format!(
            "Stale snapshot: requested {}, current {}",
            params.snapshot, current
        ));
    }

    // Extract the Node from the Declaration | Node union
    let node = params
        .arg
        .node()
        .ok_or("Synthesized declarations have no source node")?;

    // Get the first project database
    let Some(db) = databases.first() else {
        return serde_json::to_value(Option::<Type>::None).map_err(|e| e.to_string());
    };

    // Convert URI to file path
    let file_path = node
        .uri
        .to_file_path()
        .map_err(|()| "Invalid file URI".to_string())?;
    let system_path =
        SystemPathBuf::from_path_buf(file_path).map_err(|_| "Invalid path".to_string())?;

    // Get the file
    let file = system_path_to_file(db, &system_path).map_err(|_| "File not found".to_string())?;

    // Convert LSP position to byte offset
    let offset = lsp_position_to_offset(db, file, node.range.start);
    let type_info_result = type_info(db, file, offset);

    let result: Option<Type> = type_info_result.map(|info| {
        let mut next_id = 2;
        convert_type_info_recursive(db, &info.value, 1, &mut next_id)
    });

    // Return Type | undefined directly
    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Handle the `typeServer/getExpectedType` request.
///
/// Returns the expected/contextual type for an expression (from assignment, parameter, etc.).
///
/// Per the TSP protocol, returns `Type | undefined` directly.
pub(crate) fn handle_get_expected_type(
    params: &GetExpectedTypeParams,
    snapshot_manager: &SnapshotManager,
    databases: &[ProjectDatabase],
) -> Result<serde_json::Value, String> {
    // Validate the snapshot
    let current = snapshot_manager.current();
    if params.snapshot != current {
        return Err(format!(
            "Stale snapshot: requested {}, current {}",
            params.snapshot, current
        ));
    }

    // Extract the Node from the Declaration | Node union
    let node = params
        .arg
        .node()
        .ok_or("Synthesized declarations have no source node")?;

    // Get the first project database
    let Some(db) = databases.first() else {
        return serde_json::to_value(Option::<Type>::None).map_err(|e| e.to_string());
    };

    // Convert URI to file path
    let file_path = node
        .uri
        .to_file_path()
        .map_err(|()| "Invalid file URI".to_string())?;
    let system_path =
        SystemPathBuf::from_path_buf(file_path).map_err(|_| "Invalid path".to_string())?;

    // Get the file
    let file = system_path_to_file(db, &system_path).map_err(|_| "File not found".to_string())?;

    // Convert LSP position to byte offset
    let offset = lsp_position_to_offset(db, file, node.range.start);

    // Use ty_ide::expected_type_info to get the contextual/expected type
    let expected_type_result = expected_type_info(db, file, offset);

    let result: Option<Type> = expected_type_result.map(|info| {
        let mut next_id = 2;
        convert_type_info_recursive(db, &info.value, 1, &mut next_id)
    });

    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Handle the `typeServer/getDeclaredType` request.
///
/// Returns the declared type for a symbol (from explicit type annotation).
///
/// Per the TSP protocol, returns `Type | undefined` directly.
pub(crate) fn handle_get_declared_type(
    params: &GetDeclaredTypeParams,
    snapshot_manager: &SnapshotManager,
    databases: &[ProjectDatabase],
) -> Result<serde_json::Value, String> {
    // Validate the snapshot
    let current = snapshot_manager.current();
    if params.snapshot != current {
        return Err(format!(
            "Stale snapshot: requested {}, current {}",
            params.snapshot, current
        ));
    }

    // Extract the Node from the Declaration | Node union
    let node = params
        .arg
        .node()
        .ok_or("Synthesized declarations have no source node")?;

    // Get the first project database
    let Some(db) = databases.first() else {
        return serde_json::to_value(Option::<Type>::None).map_err(|e| e.to_string());
    };

    // Convert URI to file path
    let file_path = node
        .uri
        .to_file_path()
        .map_err(|()| "Invalid file URI".to_string())?;
    let system_path =
        SystemPathBuf::from_path_buf(file_path).map_err(|_| "Invalid path".to_string())?;

    // Get the file
    let file = system_path_to_file(db, &system_path).map_err(|_| "File not found".to_string())?;

    // Convert LSP position to byte offset
    let offset = lsp_position_to_offset(db, file, node.range.start);

    // Use ty_ide::declared_type_info to get the annotated type
    let declared_type_result = declared_type_info(db, file, offset);

    let result: Option<Type> = declared_type_result.map(|info| {
        let mut next_id = 2;
        convert_type_info_recursive(db, &info.value, 1, &mut next_id)
    });

    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Resolve a `ty_ide` `DeclarationInfo` into a TSP `ResolvedDeclaration`.
///
/// Converts the `NavigationTarget` (File + `TextRange`) to a TSP Declaration
/// with a URI and LSP Range. Handles both system files and vendored
/// typeshed files (which need mapping through the extraction cache).
fn resolve_declaration(
    db: &ProjectDatabase,
    declaration: Option<&DeclarationInfo>,
    category: TyCategory,
) -> Option<ResolvedDeclaration> {
    let decl_info = declaration?;
    let target = &decl_info.target;

    let file = target.file();

    // Convert File → URI
    let file_path = file.path(db);
    let uri = file_path
        .as_system_path()
        .and_then(|p| Url::from_file_path(p.as_std_path()).ok())
        .or_else(|| {
            // Fall back to vendored path mapping for typeshed stubs.
            file_path.as_vendored_path().and_then(|vp| {
                typeshed_cache::vendored_path_to_disk(vp.as_str())
                    .and_then(|d| Url::from_file_path(&d).ok())
            })
        })?;

    // Convert TextRange → LSP Range using line index
    let source = source_text(db, file);
    let index = line_index(db, file);
    let encoding = RuffPositionEncoding::Utf16;

    // Use full_range for the Declaration's LSP range. This is critical because
    // Pyright's fromProtocolNode walker matches node.start === range.start exactly,
    // and FunctionNode.start is at the `def` keyword (or first `@decorator`), NOT
    // at the function name. Using focus_range (name position) would fail to find
    // the FunctionNode, causing declaration resolution to fall back incorrectly.
    let full_range = target.full_range();
    let start_loc = index.source_location(full_range.start(), source.as_str(), encoding);
    let end_loc = index.source_location(full_range.end(), source.as_str(), encoding);

    #[allow(clippy::cast_possible_truncation)]
    let lsp_range = lsp_types::Range {
        start: lsp_types::Position {
            line: start_loc.line.to_zero_indexed() as u32,
            character: start_loc.character_offset.to_zero_indexed() as u32,
        },
        end: lsp_types::Position {
            line: end_loc.line.to_zero_indexed() as u32,
            character: end_loc.character_offset.to_zero_indexed() as u32,
        },
    };

    // Extract name from source text at focus range (the identifier name, not the full definition)
    let focus_range = target.focus_range();
    let name = source
        .as_str()
        .get(focus_range.start().into()..focus_range.end().into())
        .map(std::string::ToString::to_string);

    // Map ty category to TSP declaration category
    let decl_category = match category {
        TyCategory::Class | TyCategory::SubclassOf => DeclarationCategory::Class,
        TyCategory::Instance
        | TyCategory::Property
        | TyCategory::TypeGuard
        | TyCategory::Literal
        | TyCategory::Tuple
        | TyCategory::TypedDict
        | TyCategory::SpecialForm
        | TyCategory::NewType => DeclarationCategory::Class,
        TyCategory::Function
        | TyCategory::BoundMethod
        | TyCategory::Callable
        | TyCategory::OverloadedFunction => DeclarationCategory::Function,
        TyCategory::TypeVar => DeclarationCategory::TypeParam,
        TyCategory::TypeAlias => DeclarationCategory::TypeAlias,
        TyCategory::Module => DeclarationCategory::Import,
        _ => DeclarationCategory::Variable,
    };

    let declaration = Declaration::regular(decl_category, uri, lsp_range, name.clone());
    Some(ResolvedDeclaration { declaration, name })
}

/// Convert a `TypeInfo` to a TSP `Type`, handling union members recursively.
///
/// For union types, each member `TypeInfo` is recursively converted to an inline
/// `Type` object with its own ID and resolved declaration. The resulting
/// `Vec<Type>` is passed to `convert_type_info` as `union_members`.
///
/// IDs are assigned sequentially starting from `next_id`. The function returns
/// the converted `Type` and the next available ID (so callers can continue
/// assigning unique IDs if needed).
fn convert_type_info_recursive(
    db: &ProjectDatabase,
    info: &TypeInfo,
    id: TypeId,
    next_id: &mut TypeId,
) -> Type {
    // Convert union members first (if any)
    let union_members = info.union_members.as_ref().map(|members| {
        members
            .iter()
            .map(|member_info| {
                let member_id = *next_id;
                *next_id += 1;
                convert_type_info_recursive(db, member_info, member_id, next_id)
            })
            .collect()
    });

    // Convert overload members (if any) — each @overload arm becomes an inline Type
    let overload_members = info.overload_members.as_ref().map(|members| {
        members
            .iter()
            .map(|member_info| {
                let member_id = *next_id;
                *next_id += 1;
                convert_type_info_recursive(db, member_info, member_id, next_id)
            })
            .collect()
    });

    // Convert implementation member (if any)
    let implementation_member = info.implementation_member.as_ref().map(|impl_info| {
        let impl_id = *next_id;
        *next_id += 1;
        Box::new(convert_type_info_recursive(db, impl_info, impl_id, next_id))
    });

    let resolved = resolve_declaration(db, info.type_definition.as_ref(), info.category);
    convert_type_info(
        id,
        info.category,
        &info.display,
        info.signature.as_deref(),
        resolved,
        union_members,
        overload_members,
        implementation_member,
    )
}

/// Convert an LSP position to a byte offset in the source text.
fn lsp_position_to_offset(
    db: &ProjectDatabase,
    file: ruff_db::files::File,
    position: lsp_types::Position,
) -> TextSize {
    let source = source_text(db, file);
    let index = line_index(db, file);

    // Use UTF-16 encoding (default LSP encoding)
    let encoding = RuffPositionEncoding::Utf16;

    // Create source location from LSP position (0-indexed line and column)
    let line = ruff_source_file::OneIndexed::from_zero_indexed(position.line as usize);
    let column = ruff_source_file::OneIndexed::from_zero_indexed(position.character as usize);

    let source_location = ruff_source_file::SourceLocation {
        line,
        character_offset: column,
    };

    index.offset(source_location, source.as_str(), encoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range, Url};
    use tsp_types::{DeclarationOrNode, Node};

    fn make_test_node() -> Node {
        Node {
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
        }
    }

    #[test]
    fn test_get_computed_type_validates_snapshot() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetComputedTypeParams {
            arg: DeclarationOrNode::Node(make_test_node()),
            snapshot: 999,
        };

        let result = handle_get_computed_type(&params, &snapshot_manager, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stale snapshot"));
    }

    #[test]
    fn test_get_computed_type_returns_none_without_database() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetComputedTypeParams {
            arg: DeclarationOrNode::Node(make_test_node()),
            snapshot: snapshot_manager.current(),
        };

        let result = handle_get_computed_type(&params, &snapshot_manager, &[]);
        assert!(result.is_ok());

        // Protocol returns Type | undefined - should be null
        let parsed: Option<Type> = serde_json::from_value(result.unwrap()).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn test_get_expected_type_validates_snapshot() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetExpectedTypeParams {
            arg: DeclarationOrNode::Node(make_test_node()),
            snapshot: 999,
        };

        let result = handle_get_expected_type(&params, &snapshot_manager, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stale snapshot"));
    }

    #[test]
    fn test_get_expected_type_returns_none() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetExpectedTypeParams {
            arg: DeclarationOrNode::Node(make_test_node()),
            snapshot: snapshot_manager.current(),
        };

        let result = handle_get_expected_type(&params, &snapshot_manager, &[]);
        assert!(result.is_ok());

        let parsed: Option<Type> = serde_json::from_value(result.unwrap()).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn test_get_declared_type_validates_snapshot() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetDeclaredTypeParams {
            arg: DeclarationOrNode::Node(make_test_node()),
            snapshot: 999,
        };

        let result = handle_get_declared_type(&params, &snapshot_manager, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stale snapshot"));
    }

    #[test]
    fn test_get_declared_type_returns_none() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetDeclaredTypeParams {
            arg: DeclarationOrNode::Node(make_test_node()),
            snapshot: snapshot_manager.current(),
        };

        let result = handle_get_declared_type(&params, &snapshot_manager, &[]);
        assert!(result.is_ok());

        let parsed: Option<Type> = serde_json::from_value(result.unwrap()).unwrap();
        assert!(parsed.is_none());
    }

    // Category conversion tests moved to type_serializer module (convert_category tests).
}
