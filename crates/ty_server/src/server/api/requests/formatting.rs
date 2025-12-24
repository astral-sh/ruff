use std::borrow::Cow;

use lsp_types::{DocumentFormattingParams, TextEdit, Url, request::Formatting};
use ty_project::ProjectDatabase;

use ty_ide::{FormatData, FormattingOptions, formatting};

use crate::{
    server::api::{
        RequestHandler,
        traits::{BackgroundDocumentRequestHandler, RetriableRequestHandler},
    },
    session::{DocumentSnapshot, client::Client},
};

pub(crate) struct FormattingRequestHandler;

impl RequestHandler for FormattingRequestHandler {
    type RequestType = Formatting;
}

impl BackgroundDocumentRequestHandler for FormattingRequestHandler {
    fn document_url(params: &DocumentFormattingParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }
    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: DocumentFormattingParams,
    ) -> crate::server::Result<Option<Vec<TextEdit>>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let options = FormattingOptions::new()
            .with_indent_width(params.options.tab_size)
            .prefer_space(params.options.insert_spaces);
        println!("hello");
        let Some(FormatData { code, range }) = formatting(db, file, options) else {
            return Ok(None);
        };

        let start = range.start();
        let end = range.end();
        Ok(Some(vec![TextEdit {
            range: lsp_types::Range {
                start: lsp_types::Position {
                    line: start.line,
                    character: start.character,
                },
                end: lsp_types::Position {
                    line: end.line,
                    character: end.character,
                },
            },
            new_text: code,
        }]))
    }
}

impl RetriableRequestHandler for FormattingRequestHandler {
    const RETRY_ON_CANCELLATION: bool = true;
}
