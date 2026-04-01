//! Handler for `typeServer/getSnapshot`.

use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::tsp::requests::GetSnapshot;

pub(crate) struct GetSnapshotHandler;

impl RequestHandler for GetSnapshotHandler {
    type RequestType = GetSnapshot;
}

impl SyncRequestHandler for GetSnapshotHandler {
    fn run(session: &mut Session, _client: &Client, _params: ()) -> crate::server::Result<u64> {
        Ok(session.revision())
    }
}

/// Validate that a client-provided snapshot matches the current session revision.
///
/// Returns `Ok(())` if the snapshot is current, or a `ServerCancelled` error
/// if the snapshot is stale.
pub(crate) fn validate_snapshot(
    client_snapshot: u64,
    session_revision: u64,
) -> Result<(), lsp_server::ResponseError> {
    if client_snapshot != session_revision {
        Err(lsp_server::ResponseError {
            code: lsp_server::ErrorCode::ServerCancelled as i32,
            message: format!("Snapshot {client_snapshot} is stale (current: {session_revision})"),
            data: None,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_snapshot_current() {
        assert!(validate_snapshot(42, 42).is_ok());
    }

    #[test]
    fn validate_snapshot_stale() {
        let err = validate_snapshot(41, 42).unwrap_err();
        assert_eq!(err.code, lsp_server::ErrorCode::ServerCancelled as i32);
        assert!(err.message.contains("stale"));
    }
}
