use std::borrow::Cow;
use std::time::Instant;

use lsp_types::request::Completion;
use lsp_types::{CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Url};
use ruff_db::source::{line_index, source_text};
use ty_ide::completion;
use ty_project::ProjectDatabase;
use ty_python_semantic::CompletionKind;

use crate::document::PositionExt;
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
    fn document_url(params: &CompletionParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document_position.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
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

        let Some(file) = snapshot.file(db) else {
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);
        let offset = params.text_document_position.position.to_text_size(
            &source,
            &line_index,
            snapshot.encoding(),
        );
        let completions = completion(db, file, offset);
        if completions.is_empty() {
            return Ok(None);
        }

        let max_index_len = completions.len().saturating_sub(1).to_string().len();
        let items: Vec<CompletionItem> = completions
            .into_iter()
            .enumerate()
            .map(|(i, comp)| {
                let kind = comp.kind(db).map(ty_kind_to_lsp_kind);
                CompletionItem {
                    label: comp.name.into(),
                    kind,
                    sort_text: Some(format!("{i:-max_index_len$}")),
                    ..Default::default()
                }
            })
            .collect();
        let len = items.len();
        let response = CompletionResponse::Array(items);
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
