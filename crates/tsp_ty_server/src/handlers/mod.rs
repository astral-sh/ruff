//! TSP request handlers.
//!
//! This module contains the handlers for all TSP requests. Each handler
//! takes the request parameters and returns the appropriate response.

pub(crate) mod imports;
pub(crate) mod snapshot;
pub(crate) mod types;
pub(crate) mod version;

use tsp_types::methods;
use ty_project::ProjectDatabase;

use crate::SnapshotManager;

/// Dispatch a TSP request to the appropriate handler.
///
/// Returns `Some(response)` if the method is a TSP method, `None` otherwise.
///
/// The `databases` slice contains read-only snapshots of all project databases.
/// Handlers that need database access should use the first available database.
pub(crate) fn dispatch_tsp_request(
    method: &str,
    params: serde_json::Value,
    snapshot_manager: &SnapshotManager,
    databases: &[ProjectDatabase],
) -> Option<Result<serde_json::Value, String>> {
    match method {
        methods::GET_SUPPORTED_PROTOCOL_VERSION => {
            Some(version::handle_get_supported_protocol_version())
        }
        methods::GET_SNAPSHOT => Some(snapshot::handle_get_snapshot(snapshot_manager)),
        methods::GET_PYTHON_SEARCH_PATHS => {
            Some(parse_and_call(params, |p| {
                imports::handle_get_python_search_paths(&p, snapshot_manager, databases)
            }))
        }
        methods::RESOLVE_IMPORT => {
            Some(parse_and_call(params, |p| {
                imports::handle_resolve_import(&p, snapshot_manager, databases)
            }))
        }
        methods::GET_COMPUTED_TYPE => {
            Some(parse_and_call(params, |p| {
                types::handle_get_computed_type(&p, snapshot_manager, databases)
            }))
        }
        methods::GET_EXPECTED_TYPE => {
            Some(parse_and_call(params, |p| {
                types::handle_get_expected_type(&p, snapshot_manager, databases)
            }))
        }
        methods::GET_DECLARED_TYPE => {
            Some(parse_and_call(params, |p| {
                types::handle_get_declared_type(&p, snapshot_manager, databases)
            }))
        }
        _ if tsp_types::is_tsp_method(method) => {
            // Unknown TSP method
            Some(Err(format!("Unknown TSP method: {method}")))
        }
        _ => {
            // Not a TSP method, should be forwarded to ty_server
            None
        }
    }
}

/// Parse JSON parameters and call a handler.
///
/// This helper function parses the JSON params into the expected type T,
/// then calls the provided handler function.
fn parse_and_call<T, F>(params: serde_json::Value, handler: F) -> Result<serde_json::Value, String>
where
    T: serde::de::DeserializeOwned,
    F: FnOnce(T) -> Result<serde_json::Value, String>,
{
    let parsed: T = serde_json::from_value(params).map_err(|e| {
        format!("Failed to parse request parameters: {e}")
    })?;
    handler(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_get_supported_protocol_version() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::GET_SUPPORTED_PROTOCOL_VERSION,
            serde_json::json!({}),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_get_snapshot() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::GET_SNAPSHOT,
            serde_json::json!({}),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_get_python_search_paths() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::GET_PYTHON_SEARCH_PATHS,
            serde_json::json!({ "fromUri": "file:///workspace", "snapshot": 1 }),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_resolve_import() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::RESOLVE_IMPORT,
            serde_json::json!({
                "sourceUri": "file:///test.py",
                "moduleDescriptor": {
                    "leadingDots": 0,
                    "nameParts": ["os"]
                },
                "snapshot": 1
            }),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_get_computed_type() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::GET_COMPUTED_TYPE,
            serde_json::json!({
                "arg": {
                    "uri": "file:///test.py",
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                },
                "snapshot": 1
            }),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_get_expected_type() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::GET_EXPECTED_TYPE,
            serde_json::json!({
                "arg": {
                    "uri": "file:///test.py",
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                },
                "snapshot": 1
            }),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_get_declared_type() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            methods::GET_DECLARED_TYPE,
            serde_json::json!({
                "arg": {
                    "uri": "file:///test.py",
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                },
                "snapshot": 1
            }),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_dispatch_unknown_tsp_method() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            "typeServer/unknownMethod",
            serde_json::json!({}),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_dispatch_non_tsp_method() {
        let snapshot_manager = SnapshotManager::new();
        let result = dispatch_tsp_request(
            "textDocument/hover",
            serde_json::json!({}),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_none());
    }

    #[test]
    fn test_dispatch_invalid_params() {
        let snapshot_manager = SnapshotManager::new();
        // Missing required fields
        let result = dispatch_tsp_request(
            methods::GET_COMPUTED_TYPE,
            serde_json::json!({}),
            &snapshot_manager,
            &[],
        );

        assert!(result.is_some());
        let err = result.unwrap().unwrap_err();
        assert!(err.contains("Failed to parse"));
    }
}
