//! Handler for `typeServer/getSupportedProtocolVersion`.

use tsp_types::PROTOCOL_VERSION;

/// Handle the `typeServer/getSupportedProtocolVersion` request.
///
/// Returns the supported protocol version as a string (semver format).
/// The protocol expects a raw string like "0.4.0".
pub(crate) fn handle_get_supported_protocol_version() -> Result<serde_json::Value, String> {
    // Return the version as a plain string
    serde_json::to_value(PROTOCOL_VERSION).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_get_supported_protocol_version() {
        let result = handle_get_supported_protocol_version().unwrap();
        let parsed: String = serde_json::from_value(result).unwrap();

        // Should be a semver-like string
        assert_eq!(parsed, "0.4.0");
    }

    #[test]
    fn test_version_is_string() {
        let result = handle_get_supported_protocol_version().unwrap();
        assert!(
            result.is_string(),
            "Protocol version should be a raw string"
        );
    }
}
