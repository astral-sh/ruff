//! Handlers for import-related TSP requests.
//!
//! - `typeServer/getPythonSearchPaths`
//! - `typeServer/resolveImport`

use lsp_types::Url;
use tsp_types::{GetPythonSearchPathsParams, ResolveImportParams};
use ty_module_resolver::{ModuleName as TyModuleName, ModuleResolveMode, search_paths};
use ty_project::ProjectDatabase;

use crate::SnapshotManager;
use crate::typeshed_cache;

/// Handle the `typeServer/getPythonSearchPaths` request.
///
/// Returns the list of Python search paths used for module resolution.
/// Per the TSP protocol, this returns `string[] | undefined` (array of path strings).
///
/// This includes:
/// - System search paths from ty's module resolver
/// - The extracted vendored typeshed stdlib path (so Pylance can access builtins.pyi etc.)
///
/// # Arguments
///
/// * `params` - The request parameters, including `fromUri` and `snapshot`.
/// * `snapshot_manager` - The snapshot manager for validating the snapshot.
/// * `databases` - Read-only project database snapshots.
///
/// # Returns
///
/// An array of path strings, or an error if the snapshot is stale.
pub(crate) fn handle_get_python_search_paths(
    params: &GetPythonSearchPathsParams,
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

    // Get search paths from the first project database
    // Protocol expects string[] - array of URI strings
    let mut search_paths_result: Vec<String> = if let Some(db) = databases.first() {
        search_paths(db, ModuleResolveMode::StubsAllowed)
            .filter_map(|path| {
                // Convert system path to URL string
                path.as_system_path()
                    .and_then(|p| Url::from_file_path(p.as_std_path()).ok())
                    .map(|url| url.to_string())
            })
            .collect()
    } else {
        Vec::new()
    };

    // Include the extracted vendored typeshed stdlib path.
    // ty embeds typeshed in the binary; this extracts it to disk so Pylance
    // can resolve declarations in builtins.pyi, os.pyi, etc.
    if let Some(stdlib_path) = typeshed_cache::extracted_typeshed_stdlib_path() {
        if let Ok(url) = Url::from_file_path(&stdlib_path) {
            let url_str = url.to_string();
            // Only add if not already present (avoid duplicates)
            if !search_paths_result.contains(&url_str) {
                search_paths_result.push(url_str);
            }
        }
    }

    // Return the array of strings directly (protocol expects string[] | undefined)
    serde_json::to_value(search_paths_result).map_err(|e| e.to_string())
}

/// Handle the `typeServer/resolveImport` request.
///
/// Resolves a module name to a file path.
/// Per the TSP protocol, this returns `string | undefined` (URI string or null).
///
/// # Arguments
///
/// * `params` - The request parameters, including `sourceUri`, `moduleDescriptor`, and `snapshot`.
/// * `snapshot_manager` - The snapshot manager for validating the snapshot.
/// * `databases` - Read-only project database snapshots.
///
/// # Returns
///
/// The resolved URI string, or null if not found. Error if snapshot is stale.
pub(crate) fn handle_resolve_import(
    params: &ResolveImportParams,
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

    // Try to resolve the import using the first project database
    // Protocol expects string | undefined - just the URI or null
    let result: Option<String> = if let Some(db) = databases.first() {
        // Convert ModuleDescriptor to a dotted module name string
        let module_name_str = params.module_descriptor.to_module_name();

        // Parse the module name
        if let Some(module_name) = TyModuleName::new(&module_name_str) {
            // Resolve without a specific importing file (confident resolution)
            if let Some(module) = ty_module_resolver::resolve_module_confident(db, &module_name) {
                // Get the file from the resolved module
                if let Some(file) = module.file(db) {
                    // Get file path and convert to URI string
                    let file_path = file.path(db);
                    // Try system path first (user/site-packages files)
                    let uri = file_path
                        .as_system_path()
                        .and_then(|p| Url::from_file_path(p.as_std_path()).ok())
                        .or_else(|| {
                            // Fall back to vendored path mapping for typeshed stubs.
                            // ty resolves stdlib modules to vendored paths (e.g., "stdlib/builtins.pyi").
                            // We map those to the on-disk extracted typeshed cache.
                            file_path.as_vendored_path().and_then(|vp| {
                                typeshed_cache::vendored_path_to_disk(vp.as_str())
                                    .and_then(|d| Url::from_file_path(&d).ok())
                            })
                        });
                    uri.map(|u| u.to_string())
                } else {
                    // Namespace package - no file
                    None
                }
            } else {
                // Module not found
                None
            }
        } else {
            // Invalid module name
            None
        }
    } else {
        // No database available
        None
    };

    // Return the URI string directly, or null (protocol expects string | undefined)
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsp_types::ModuleDescriptor;

    #[test]
    fn test_get_python_search_paths_validates_snapshot() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetPythonSearchPathsParams {
            from_uri: "file:///workspace".to_string(),
            snapshot: 999,
        };

        let result = handle_get_python_search_paths(&params, &snapshot_manager, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stale snapshot"));
    }

    #[test]
    fn test_get_python_search_paths_returns_extracted_typeshed_without_database() {
        let snapshot_manager = SnapshotManager::new();
        let params = GetPythonSearchPathsParams {
            from_uri: "file:///workspace".to_string(),
            snapshot: snapshot_manager.current(),
        };

        let result = handle_get_python_search_paths(&params, &snapshot_manager, &[]);
        assert!(result.is_ok());

        // Protocol returns string[] - parse as Vec<String>
        // Even without a database, the extracted typeshed stdlib should be present
        let parsed: Vec<String> = serde_json::from_value(result.unwrap()).unwrap();
        assert!(
            parsed.len() == 1,
            "Should have exactly 1 path (extracted typeshed stdlib), got: {parsed:?}"
        );
        assert!(
            parsed[0].contains("tsp-ty-typeshed"),
            "Path should contain 'tsp-ty-typeshed': {}",
            parsed[0]
        );
    }

    #[test]
    fn test_resolve_import_validates_snapshot() {
        let snapshot_manager = SnapshotManager::new();
        let params = ResolveImportParams {
            source_uri: "file:///test.py".to_string(),
            module_descriptor: ModuleDescriptor {
                leading_dots: 0,
                name_parts: vec!["os".to_string()],
            },
            snapshot: 999,
        };

        let result = handle_resolve_import(&params, &snapshot_manager, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Stale snapshot"));
    }

    #[test]
    fn test_resolve_import_returns_not_resolved() {
        let snapshot_manager = SnapshotManager::new();
        let params = ResolveImportParams {
            source_uri: "file:///test.py".to_string(),
            module_descriptor: ModuleDescriptor {
                leading_dots: 0,
                name_parts: vec!["os".to_string()],
            },
            snapshot: snapshot_manager.current(),
        };

        let result = handle_resolve_import(&params, &snapshot_manager, &[]);
        assert!(result.is_ok());

        // Protocol returns string | undefined - parse as Option<String>
        let parsed: Option<String> = serde_json::from_value(result.unwrap()).unwrap();
        assert!(parsed.is_none());
    }
}
