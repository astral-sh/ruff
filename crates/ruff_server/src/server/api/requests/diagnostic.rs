use crate::server::api::diagnostics::generate_diagnostics;
use crate::session::DocumentSnapshot;
use crate::{server::Result, session::Client};
use lsp_types::{self as types, DocumentDiagnosticReport, DocumentDiagnosticRequest};
use types::{FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport};

pub(crate) struct DocumentDiagnostic;

impl super::RequestHandler for DocumentDiagnostic {
    type RequestType = DocumentDiagnosticRequest;
}

impl super::BackgroundDocumentRequestHandler for DocumentDiagnostic {
    super::define_document_url!(params: &types::DocumentDiagnosticParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _client: &Client,
        _params: types::DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReport> {
        Ok(
            DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(
                RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        // TODO(jane): eventually this will be important for caching diagnostic information.
                        result_id: None,
                        // Pull diagnostic requests are only called for text documents.
                        // Since diagnostic requests generate
                        items: generate_diagnostics(&snapshot)
                            .into_iter()
                            .next()
                            .map(|(_, diagnostics)| diagnostics)
                            .unwrap_or_default(),
                    },
                },
            ),
        )
    }
}
