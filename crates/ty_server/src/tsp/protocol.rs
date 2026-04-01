//! TSP protocol constants and method names.

/// TSP method name constants.
pub(crate) mod methods {
    pub(crate) const GET_SUPPORTED_PROTOCOL_VERSION: &str =
        "typeServer/getSupportedProtocolVersion";
    pub(crate) const GET_SNAPSHOT: &str = "typeServer/getSnapshot";
    pub(crate) const SNAPSHOT_CHANGED: &str = "typeServer/snapshotChanged";
    pub(crate) const GET_PYTHON_SEARCH_PATHS: &str = "typeServer/getPythonSearchPaths";
    pub(crate) const RESOLVE_IMPORT: &str = "typeServer/resolveImport";
    pub(crate) const GET_COMPUTED_TYPE: &str = "typeServer/getComputedType";
    pub(crate) const GET_EXPECTED_TYPE: &str = "typeServer/getExpectedType";
    pub(crate) const GET_DECLARED_TYPE: &str = "typeServer/getDeclaredType";
}

/// The current TSP protocol version.
pub(crate) const PROTOCOL_VERSION: &str = "0.4.0";

#[cfg(test)]
mod tests {
    use super::*;

    const TSP_NAMESPACE: &str = "typeServer";

    fn is_tsp_method(method: &str) -> bool {
        method.starts_with(TSP_NAMESPACE)
    }

    #[test]
    fn tsp_methods_recognized() {
        assert!(is_tsp_method(methods::GET_SNAPSHOT));
        assert!(is_tsp_method(methods::GET_COMPUTED_TYPE));
        assert!(is_tsp_method(methods::SNAPSHOT_CHANGED));
        assert!(is_tsp_method(methods::GET_PYTHON_SEARCH_PATHS));
        assert!(is_tsp_method(methods::RESOLVE_IMPORT));
        assert!(is_tsp_method(methods::GET_EXPECTED_TYPE));
        assert!(is_tsp_method(methods::GET_DECLARED_TYPE));
        assert!(is_tsp_method(methods::GET_SUPPORTED_PROTOCOL_VERSION));
    }

    #[test]
    fn non_tsp_methods_rejected() {
        assert!(!is_tsp_method("textDocument/hover"));
        assert!(!is_tsp_method("initialize"));
        assert!(!is_tsp_method(""));
    }
}
