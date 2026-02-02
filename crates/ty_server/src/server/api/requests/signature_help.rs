use std::borrow::Cow;

use crate::document::{PositionEncoding, PositionExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::request::SignatureHelpRequest;
use lsp_types::{
    Documentation, ParameterInformation, ParameterLabel, SignatureHelp, SignatureHelpParams,
    SignatureInformation, Url,
};
use ty_ide::signature_help;
use ty_project::ProjectDatabase;

pub(crate) struct SignatureHelpRequestHandler;

impl RequestHandler for SignatureHelpRequestHandler {
    type RequestType = SignatureHelpRequest;
}

impl BackgroundDocumentRequestHandler for SignatureHelpRequestHandler {
    fn document_url(params: &SignatureHelpParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: SignatureHelpParams,
    ) -> crate::server::Result<Option<SignatureHelp>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.url(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        // Extract signature help capabilities from the client
        let resolved_capabilities = snapshot.resolved_client_capabilities();

        let Some(signature_help_info) = signature_help(db, file, offset) else {
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
            .map(|sig| {
                let parameters = sig
                    .parameters
                    .into_iter()
                    .map(|param| {
                        let label = if resolved_capabilities.supports_signature_label_offset() {
                            // Find the parameter's offset in the signature label
                            if let Some(start) = sig.label.find(&param.label) {
                                let encoding = snapshot.encoding();

                                // Convert byte offsets to character offsets based on negotiated encoding
                                let start_char_offset = match encoding {
                                    PositionEncoding::UTF8 => start,
                                    PositionEncoding::UTF16 => {
                                        sig.label[..start].encode_utf16().count()
                                    }
                                    PositionEncoding::UTF32 => sig.label[..start].chars().count(),
                                };

                                let end_char_offset = match encoding {
                                    PositionEncoding::UTF8 => start + param.label.len(),
                                    PositionEncoding::UTF16 => sig.label
                                        [..start + param.label.len()]
                                        .encode_utf16()
                                        .count(),
                                    PositionEncoding::UTF32 => {
                                        sig.label[..start + param.label.len()].chars().count()
                                    }
                                };

                                let start_u32 =
                                    u32::try_from(start_char_offset).unwrap_or(u32::MAX);
                                let end_u32 = u32::try_from(end_char_offset).unwrap_or(u32::MAX);
                                ParameterLabel::LabelOffsets([start_u32, end_u32])
                            } else {
                                ParameterLabel::Simple(param.label)
                            }
                        } else {
                            ParameterLabel::Simple(param.label)
                        };

                        ParameterInformation {
                            label,
                            documentation: param.documentation.map(Documentation::String),
                        }
                    })
                    .collect();

                let active_parameter =
                    if resolved_capabilities.supports_signature_active_parameter() {
                        sig.active_parameter.and_then(|p| u32::try_from(p).ok())
                    } else {
                        None
                    };

                SignatureInformation {
                    label: sig.label,
                    documentation: sig
                        .documentation
                        .map(|docstring| Documentation::String(docstring.render_plaintext())),
                    parameters: Some(parameters),
                    active_parameter,
                }
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
