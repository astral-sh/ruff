use crate::server::api::diagnostics::generate_diagnostics;
use crate::{server::Result, session::Client};
use lsp_types::{self as types, DocumentDiagnosticReport, DocumentDiagnosticRequest};
use types::{FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport};

pub(crate) struct DocumentDiagnostic;

impl super::RequestHandler for DocumentDiagnostic {
    type RequestType = DocumentDiagnosticRequest;
}

impl super::BackgroundDocumentRequestHandler for DocumentDiagnostic {
    super::define_document_uri!(params: &types::DocumentDiagnosticParams);

    fn run_with_snapshot(
        snapshot: Self::Snapshot,
        _client: &Client,
        params: types::DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReport> {
        let diagnostics = match snapshot {
            Ok(snapshot) => generate_diagnostics(&snapshot)
                .remove(&params.text_document.uri)
                .unwrap_or_default(),
            Err(uri) => {
                tracing::warn!("Returning no diagnostics because document `{uri}` isn't open.");
                Vec::new()
            }
        };

        Ok(RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                // TODO(jane): eventually this will be important for caching diagnostic information.
                result_id: None,
                items: diagnostics,
            },
        }
        .into())
    }
}
