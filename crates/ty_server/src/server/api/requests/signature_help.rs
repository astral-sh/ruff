use std::borrow::Cow;

use crate::document::{PositionEncoding, PositionExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::{ActiveParameter, SignatureHelpRequest};
use lsp_types::{
    Documentation, ParameterInformation, ParameterInformationLabel, SignatureHelp,
    SignatureHelpParams, SignatureInformation, Uri,
};
use ty_ide::{MarkupKind, signature_help};
use ty_project::{ProjectDatabase, SemanticDb as _};

pub(crate) struct SignatureHelpRequestHandler;

impl RequestHandler for SignatureHelpRequestHandler {
    type RequestType = SignatureHelpRequest;
}

impl BackgroundDocumentRequestHandler for SignatureHelpRequestHandler {
    fn document_uri(params: &SignatureHelpParams) -> Cow<'_, Uri> {
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

        let Some(file) = snapshot.document().to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.document().uri(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        // Extract signature help capabilities from the client
        let resolved_capabilities = snapshot.resolved_client_capabilities();

        let Some(signature_help_info) = signature_help(db, db.program_file(file), offset) else {
            return Ok(None);
        };

        // Compute active parameter from the active signature
        let active_parameter = signature_help_info
            .active_signature
            .and_then(|s| signature_help_info.signatures.get(s))
            .and_then(|sig| sig.active_parameter)
            .and_then(|p| u32::try_from(p).ok())
            .map(ActiveParameter::Int);

        // Convert from IDE types to LSP types
        let signatures = signature_help_info
            .signatures
            .into_iter()
            .map(|sig| {
                let offset_encoding = resolved_capabilities
                    .supports_signature_label_offset()
                    .then(|| {
                        let encoding = snapshot.encoding();
                        // ASCII byte offsets count code units in all three encodings.
                        if encoding == PositionEncoding::UTF8 || sig.label.is_ascii() {
                            PositionEncoding::UTF8
                        } else {
                            encoding
                        }
                    });
                let parameters = sig
                    .parameters
                    .into_iter()
                    .map(|param| {
                        let label = if let Some(encoding) = offset_encoding
                            && let Some(start) = sig.label.find(&param.label)
                        {
                            let len = |text: &str| match encoding {
                                PositionEncoding::UTF8 => text.len(),
                                PositionEncoding::UTF16 => text.encode_utf16().count(),
                                PositionEncoding::UTF32 => text.chars().count(),
                            };
                            let start = len(&sig.label[..start]);
                            let end = start + len(&param.label);
                            ParameterInformationLabel::Tuple((
                                u32::try_from(start).unwrap_or(u32::MAX),
                                u32::try_from(end).unwrap_or(u32::MAX),
                            ))
                        } else {
                            ParameterInformationLabel::String(param.label)
                        };

                        ParameterInformation {
                            label,
                            documentation: param.documentation.map(Documentation::String),
                        }
                    })
                    .collect();

                let active_parameter =
                    if resolved_capabilities.supports_signature_active_parameter() {
                        sig.active_parameter
                            .and_then(|p| u32::try_from(p).ok())
                            .map(ActiveParameter::Int)
                    } else {
                        None
                    };

                SignatureInformation {
                    label: sig.label,
                    documentation: sig.documentation.map(|docstring| {
                        Documentation::String(docstring.render(MarkupKind::PlainText))
                    }),
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
