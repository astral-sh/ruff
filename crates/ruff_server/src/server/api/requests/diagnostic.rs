use crate::server::api::diagnostics::generate_diagnostics;
use crate::{server::Result, session::Client};
use lsp_types::{self as types, request as req};
use types::{
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
    RelatedFullDocumentDiagnosticReport,
};

pub(crate) struct DocumentDiagnostic;

impl super::RequestHandler for DocumentDiagnostic {
    type RequestType = req::DocumentDiagnosticRequest;
}

impl super::BackgroundDocumentRequestHandler for DocumentDiagnostic {
    super::define_document_url!(params: &types::DocumentDiagnosticParams);

    fn run_with_snapshot(
        snapshot: Self::Snapshot,
        _client: &Client,
        _params: types::DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let diagnostics = match snapshot {
            Ok(snapshot) => generate_diagnostics(&snapshot)
                .into_iter()
                .next()
                .map(|(_, diagnostics)| diagnostics)
                .unwrap_or_default(),
            Err(url) => {
                tracing::warn!("Returning no diagnostics because document `{url}` isn't open.");
                Vec::new()
            }
        };

        Ok(DocumentDiagnosticReportResult::Report(
            types::DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    // TODO(jane): eventually this will be important for caching diagnostic information.
                    result_id: None,
                    // Pull diagnostic requests are only called for text documents.
                    // Since diagnostic requests generate
                    items: diagnostics,
                },
            }),
        ))
    }
}
