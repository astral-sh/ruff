use std::borrow::Cow;

use crate::DocumentSnapshot;
use crate::document::PositionExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::client::Client;
use lsp_types::request::SignatureHelpRequest;
use lsp_types::{
    Documentation, ParameterInformation, ParameterLabel, SignatureHelp, SignatureHelpParams,
    SignatureInformation, Url,
};
use ruff_db::source::{line_index, source_text};
use ty_ide::signature_help;
use ty_project::ProjectDatabase;

pub(crate) struct SignatureHelpRequestHandler;

impl RequestHandler for SignatureHelpRequestHandler {
    type RequestType = SignatureHelpRequest;
}

impl BackgroundDocumentRequestHandler for SignatureHelpRequestHandler {
    fn document_url(params: &SignatureHelpParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: SignatureHelpParams,
    ) -> crate::server::Result<Option<SignatureHelp>> {
        if snapshot.client_settings().is_language_services_disabled() {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            tracing::debug!("Failed to resolve file for {:?}", params);
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);
        let offset = params.text_document_position_params.position.to_text_size(
            &source,
            &line_index,
            snapshot.encoding(),
        );

        // Extract signature help capabilities from the client
        let resolved_capabilities = snapshot.resolved_client_capabilities();
        let client_capabilities = ty_ide::SignatureHelpClientCapabilities {
            signature_label_offset_support: resolved_capabilities.signature_label_offset_support,
            active_parameter_support: resolved_capabilities.signature_active_parameter_support,
        };
        let Some(signature_help_info) = signature_help(db, file, offset, &client_capabilities)
        else {
            return Ok(None);
        };

        // Compute active parameter from the active signature
        let active_parameter = signature_help_info
            .active_signature
            .and_then(|s| signature_help_info.signatures.get(s))
            .and_then(|sig| sig.active_parameter)
            .and_then(|p| u32::try_from(p).ok());

        // Convert from IDE types to LSP types
        let signatures = signature_help_info
            .signatures
            .into_iter()
            .map(|sig| SignatureInformation {
                label: sig.label,
                documentation: sig.documentation.map(Documentation::String),
                parameters: Some(
                    sig.parameters
                        .into_iter()
                        .map(|param| {
                            let label = match param.label {
                                ty_ide::ParameterLabel::String(s) => ParameterLabel::Simple(s),
                                ty_ide::ParameterLabel::Offset { start, length } => {
                                    // Convert TextSize to u32, clamping to u32::MAX if needed
                                    let start_u32 =
                                        u32::try_from(start.to_usize()).unwrap_or(u32::MAX);
                                    let end_u32 =
                                        u32::try_from(start.to_usize() + length.to_usize())
                                            .unwrap_or(u32::MAX);
                                    ParameterLabel::LabelOffsets([start_u32, end_u32])
                                }
                            };
                            ParameterInformation {
                                label,
                                documentation: param.documentation.map(Documentation::String),
                            }
                        })
                        .collect(),
                ),
                active_parameter: sig.active_parameter.and_then(|p| u32::try_from(p).ok()),
            })
            .collect();

        let signature_help = SignatureHelp {
            signatures,
            active_signature: signature_help_info
                .active_signature
                .and_then(|s| u32::try_from(s).ok()),
            active_parameter,
        };

        Ok(Some(signature_help))
    }
}

impl RetriableRequestHandler for SignatureHelpRequestHandler {}
