use crate::edit::{Replacement, ToRangeExt};
use crate::fix::Fixes;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use crate::{PositionEncoding, TextDocument};
use lsp_types::{self as types, request as req};
use ruff_python_ast::PySourceType;
use ruff_source_file::LineIndex;
use ruff_workspace::FormatterSettings;
use types::TextEdit;

pub(crate) struct Format;

impl super::RequestHandler for Format {
    type RequestType = req::Formatting;
}

impl super::BackgroundDocumentRequestHandler for Format {
    super::define_document_url!(params: &types::DocumentFormattingParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        _params: types::DocumentFormattingParams,
    ) -> Result<super::FormatResponse> {
        format_document(&snapshot)
    }
}

/// Formats either a full text document or each individual cell in a single notebook document.
pub(super) fn format_full_document(snapshot: &DocumentSnapshot) -> Result<Fixes> {
    let mut fixes = Fixes::default();

    if let Some(notebook) = snapshot.query().as_notebook() {
        for (url, text_document) in notebook
            .urls()
            .map(|url| (url.clone(), notebook.cell_document_by_uri(url).unwrap()))
        {
            if let Some(changes) = format_text_document(
                text_document,
                snapshot.query().source_type(),
                snapshot.query().settings().formatter(),
                snapshot.encoding(),
                true,
            )? {
                fixes.insert(url, changes);
            }
        }
    } else {
        if let Some(changes) = format_text_document(
            snapshot.query().as_single_document().unwrap(),
            snapshot.query().source_type(),
            snapshot.query().settings().formatter(),
            snapshot.encoding(),
            false,
        )? {
            fixes.insert(snapshot.query().make_key().into_url(), changes);
        }
    }

    Ok(fixes)
}

/// Formats either a full text document or an specific notebook cell. If the query within the snapshot is a notebook document
/// with no selected cell, this will throw an error.
pub(super) fn format_document(snapshot: &DocumentSnapshot) -> Result<super::FormatResponse> {
    let text_document = snapshot
        .query()
        .as_single_document()
        .expect("format should only be called on text documents or notebook cells");
    format_text_document(
        text_document,
        snapshot.query().source_type(),
        snapshot.query().settings().formatter(),
        snapshot.encoding(),
        snapshot.query().as_notebook().is_some(),
    )
}

fn format_text_document(
    text_document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    encoding: PositionEncoding,
    is_notebook: bool,
) -> Result<super::FormatResponse> {
    let source = text_document.contents();
    let mut formatted = crate::format::format(text_document, source_type, formatter_settings)
        .with_failure_code(lsp_server::ErrorCode::InternalError)?;
    // fast path - if the code is the same, return early
    if formatted == source {
        return Ok(None);
    }

    // special case - avoid adding a newline to a notebook cell if it didn't already exist
    if is_notebook {
        let mut trimmed = formatted.as_str();
        if !source.ends_with("\r\n") {
            trimmed = trimmed.trim_end_matches("\r\n");
        }
        if !source.ends_with('\n') {
            trimmed = trimmed.trim_end_matches('\n');
        }
        if !source.ends_with('\r') {
            trimmed = trimmed.trim_end_matches('\r');
        }
        formatted = trimmed.to_string();
    }

    let formatted_index: LineIndex = LineIndex::from_source_text(&formatted);

    let unformatted_index = text_document.index();

    let Replacement {
        source_range,
        modified_range: formatted_range,
    } = Replacement::between(
        source,
        unformatted_index.line_starts(),
        &formatted,
        formatted_index.line_starts(),
    );

    Ok(Some(vec![TextEdit {
        range: source_range.to_range(source, unformatted_index, encoding),
        new_text: formatted[formatted_range].to_owned(),
    }]))
}
