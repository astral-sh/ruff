//! TSP protocol constants and method names.
//!
//! This module defines the method names for all TSP requests and notifications.

/// The namespace for all TSP methods.
pub const TSP_NAMESPACE: &str = "typeServer";

/// TSP method names.
pub mod methods {
    /// Get the supported protocol version.
    pub const GET_SUPPORTED_PROTOCOL_VERSION: &str = "typeServer/getSupportedProtocolVersion";

    /// Get the current snapshot number.
    pub const GET_SNAPSHOT: &str = "typeServer/getSnapshot";

    /// Notification sent when the snapshot changes.
    pub const SNAPSHOT_CHANGED: &str = "typeServer/snapshotChanged";

    /// Get Python search paths.
    pub const GET_PYTHON_SEARCH_PATHS: &str = "typeServer/getPythonSearchPaths";

    /// Resolve an import.
    pub const RESOLVE_IMPORT: &str = "typeServer/resolveImport";

    /// Get the computed type of an expression.
    pub const GET_COMPUTED_TYPE: &str = "typeServer/getComputedType";

    /// Get the expected type of an expression.
    pub const GET_EXPECTED_TYPE: &str = "typeServer/getExpectedType";

    /// Get the declared type of a symbol.
    pub const GET_DECLARED_TYPE: &str = "typeServer/getDeclaredType";
}

/// Invalid handle sentinel value.
/// Used to indicate "invalid/unavailable" for handle-like fields.
pub const INVALID_HANDLE: i64 = -1;

/// The current TSP protocol version (matches TypeServerProtocol.TypeServerVersion.current).
pub const PROTOCOL_VERSION: &str = "0.4.0";

/// Check if a method name is a TSP method.
pub fn is_tsp_method(method: &str) -> bool {
    method.starts_with(TSP_NAMESPACE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tsp_method() {
        assert!(is_tsp_method("typeServer/getSnapshot"));
        assert!(is_tsp_method("typeServer/getComputedType"));
        assert!(!is_tsp_method("textDocument/hover"));
        assert!(!is_tsp_method("initialize"));
    }
}
