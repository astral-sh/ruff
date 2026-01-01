use std::borrow::Cow;
use std::time::Instant;

use lsp_types::request::Completion;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, CompletionList,
    CompletionParams, CompletionResponse, Documentation, TextEdit, Url,
};
use ruff_source_file::OneIndexed;
use ruff_text_size::Ranged;
use ty_ide::{CompletionKind, completion};
use ty_project::ProjectDatabase;

use crate::document::{PositionExt, ToRangeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct CompletionRequestHandler;

impl RequestHandler for CompletionRequestHandler {
    type RequestType = Completion;
}

impl BackgroundDocumentRequestHandler for CompletionRequestHandler {
    fn document_url(params: &CompletionParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: CompletionParams,
    ) -> crate::server::Result<Option<CompletionResponse>> {
        let start = Instant::now();

        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position.position.to_text_size(
            db,
            file,
            snapshot.url(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };
        let settings = snapshot.workspace_settings().completions();
        let completions = completion(db, settings, file, offset);
        if completions.is_empty() {
            return Ok(None);
        }

        // Safety: we just checked that completions is not empty.
        let max_index_len = OneIndexed::new(completions.len()).unwrap().digits().get();
        let items: Vec<CompletionItem> = completions
            .into_iter()
            .enumerate()
            .map(|(i, comp)| {
                let kind = comp.kind(db).map(ty_kind_to_lsp_kind);
                let type_display = comp.ty.map(|ty| ty.display(db).to_string());
                let import_edit = comp.import.as_ref().and_then(|edit| {
                    let range = edit
                        .range()
                        .to_lsp_range(db, file, snapshot.encoding())?
                        .local_range();
                    Some(TextEdit {
                        range,
                        new_text: edit.content().map(ToString::to_string).unwrap_or_default(),
                    })
                });

                let name = comp.insert.as_deref().unwrap_or(&comp.name).to_string();
                let import_suffix = comp
                    .module_name
                    .and_then(|name| import_edit.is_some().then(|| format!(" (import {name})")));
                let (label, label_details) = if snapshot
                    .resolved_client_capabilities()
                    .supports_completion_item_label_details()
                {
                    let label_details = CompletionItemLabelDetails {
                        detail: import_suffix,
                        description: type_display.clone(),
                    };
                    (name, Some(label_details))
                } else {
                    let label = import_suffix
                        .map(|suffix| format!("{name}{suffix}"))
                        .unwrap_or_else(|| name);
                    (label, None)
                };

                let documentation = comp.documentation.map(|docstring| {
                    let (kind, value) = if snapshot
                        .resolved_client_capabilities()
                        .prefers_markdown_in_completion()
                    {
                        (lsp_types::MarkupKind::Markdown, docstring.render_markdown())
                    } else {
                        (
                            lsp_types::MarkupKind::PlainText,
                            docstring.render_plaintext(),
                        )
                    };

                    Documentation::MarkupContent(lsp_types::MarkupContent { kind, value })
                });

                CompletionItem {
                    label,
                    kind,
                    sort_text: Some(format!("{i:-max_index_len$}")),
                    detail: type_display,
                    label_details,
                    insert_text: comp.insert.map(String::from),
                    additional_text_edits: import_edit.map(|edit| vec![edit]),
                    documentation,
                    ..Default::default()
                }
            })
            .collect();
        let len = items.len();
        let response = CompletionResponse::List(CompletionList {
            is_incomplete: true,
            items,
        });
        tracing::debug!(
            "Completions request returned {len} suggestions in {elapsed:?}",
            elapsed = Instant::now().duration_since(start)
        );
        Ok(Some(response))
    }
}

impl RetriableRequestHandler for CompletionRequestHandler {
    const RETRY_ON_CANCELLATION: bool = true;
}

fn ty_kind_to_lsp_kind(kind: CompletionKind) -> CompletionItemKind {
    // Gimme my dang globs in tight scopes!
    #[allow(clippy::enum_glob_use)]
    use self::CompletionKind::*;

    // ref https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#completionItemKind
    match kind {
        Text => CompletionItemKind::TEXT,
        Method => CompletionItemKind::METHOD,
        Function => CompletionItemKind::FUNCTION,
        Constructor => CompletionItemKind::CONSTRUCTOR,
        Field => CompletionItemKind::FIELD,
        Variable => CompletionItemKind::VARIABLE,
        Class => CompletionItemKind::CLASS,
        Interface => CompletionItemKind::INTERFACE,
        Module => CompletionItemKind::MODULE,
        Property => CompletionItemKind::PROPERTY,
        Unit => CompletionItemKind::UNIT,
        Value => CompletionItemKind::VALUE,
        Enum => CompletionItemKind::ENUM,
        Keyword => CompletionItemKind::KEYWORD,
        Snippet => CompletionItemKind::SNIPPET,
        Color => CompletionItemKind::COLOR,
        File => CompletionItemKind::FILE,
        Reference => CompletionItemKind::REFERENCE,
        Folder => CompletionItemKind::FOLDER,
        EnumMember => CompletionItemKind::ENUM_MEMBER,
        Constant => CompletionItemKind::CONSTANT,
        Struct => CompletionItemKind::STRUCT,
        Event => CompletionItemKind::EVENT,
        Operator => CompletionItemKind::OPERATOR,
        TypeParameter => CompletionItemKind::TYPE_PARAMETER,
    }
}
