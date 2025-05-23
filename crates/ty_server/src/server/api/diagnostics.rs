use lsp_server::ErrorCode;
use lsp_types::{PublishDiagnosticsParams, Url, notification::PublishDiagnostics};

use super::LSPResult;
use crate::client::Client;
use crate::server::Result;

pub(super) fn clear_diagnostics(uri: &Url, client: &Client) -> Result<()> {
    client
        .send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![],
            version: None,
        })
        .with_failure_code(ErrorCode::InternalError)?;
    Ok(())
}
