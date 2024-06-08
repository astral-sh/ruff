use std::{borrow::Cow, sync::Arc};

use rustc_hash::FxHashMap;

use ruff_linter::{
    linter::{FixerResult, LinterResult},
    packaging::detect_package_root,
    settings::{flags, types::UnsafeFixes, LinterSettings},
};
use ruff_notebook::SourceValue;
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_workspace::resolver::match_any_exclusion;

use crate::{
    edit::{Replacement, ToRangeExt},
    session::DocumentQuery,
    PositionEncoding, TextDocument,
};

/// A simultaneous fix made across a single text document or among an arbitrary
/// number of notebook cells.
pub(crate) type Fixes = FxHashMap<lsp_types::Url, Vec<lsp_types::TextEdit>>;

pub(crate) fn fix_all(
    query: &DocumentQuery,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<Fixes> {
    let source_kind = query.make_source_kind();

    let file_resolver_settings = query.settings().file_resolver();
    let document_path = query.file_path();

    // If the document is excluded, return an empty list of fixes.
    let package = if let Some(document_path) = document_path.as_ref() {
        if let Some(exclusion) = match_any_exclusion(
            document_path,
            &file_resolver_settings.exclude,
            &file_resolver_settings.extend_exclude,
            Some(&linter_settings.exclude),
            None,
        ) {
            tracing::debug!(
                "Ignored path via `{}`: {}",
                exclusion,
                document_path.display()
            );
            return Ok(Fixes::default());
        }

        detect_package_root(
            document_path
                .parent()
                .expect("a path to a document should have a parent path"),
            &linter_settings.namespace_packages,
        )
    } else {
        None
    };

    let source_type = query.source_type();

    // We need to iteratively apply all safe fixes onto a single file and then
    // create a diff between the modified file and the original source to use as a single workspace
    // edit.
    // If we simply generated the diagnostics with `check_path` and then applied fixes individually,
    // there's a possibility they could overlap or introduce new problems that need to be fixed,
    // which is inconsistent with how `ruff check --fix` works.
    let FixerResult {
        transformed,
        result: LinterResult { error, .. },
        ..
    } = ruff_linter::linter::lint_fix(
        query.virtual_file_path(),
        package,
        flags::Noqa::Enabled,
        UnsafeFixes::Disabled,
        linter_settings,
        &source_kind,
        source_type,
    )?;

    if let Some(error) = error {
        // abort early if a parsing error occurred
        return Err(anyhow::anyhow!(
            "A parsing error occurred during `fix_all`: {error}"
        ));
    }

    // fast path: if `transformed` is still borrowed, no changes were made and we can return early
    if let Cow::Borrowed(_) = transformed {
        return Ok(Fixes::default());
    }

    if let (Some(source_notebook), Some(modified_notebook)) =
        (source_kind.as_ipy_notebook(), transformed.as_ipy_notebook())
    {
        fn cell_source(cell: &ruff_notebook::Cell) -> String {
            match cell.source() {
                SourceValue::String(string) => string.clone(),
                SourceValue::StringArray(array) => array.join(""),
            }
        }

        let Some(notebook) = query.as_notebook() else {
            anyhow::bail!("Notebook document expected from notebook source kind");
        };
        let mut fixes = Fixes::default();
        for ((source, modified), url) in source_notebook
            .cells()
            .iter()
            .map(cell_source)
            .zip(modified_notebook.cells().iter().map(cell_source))
            .zip(notebook.urls())
        {
            let source_index = LineIndex::from_source_text(&source);
            let modified_index = LineIndex::from_source_text(&modified);

            let Replacement {
                source_range,
                modified_range,
            } = Replacement::between(
                &source,
                source_index.line_starts(),
                &modified,
                modified_index.line_starts(),
            );

            fixes.insert(
                url.clone(),
                vec![lsp_types::TextEdit {
                    range: source_range.to_range(
                        source_kind.source_code(),
                        &source_index,
                        encoding,
                    ),
                    new_text: modified[modified_range].to_owned(),
                }],
            );
        }
        Ok(fixes)
    } else {
        let source_index = LineIndex::from_source_text(source_kind.source_code());

        let modified = transformed.source_code();
        let modified_index = LineIndex::from_source_text(modified);

        let Replacement {
            source_range,
            modified_range,
        } = Replacement::between(
            source_kind.source_code(),
            source_index.line_starts(),
            modified,
            modified_index.line_starts(),
        );
        Ok([(
            query.make_key().into_url(),
            vec![lsp_types::TextEdit {
                range: source_range.to_range(source_kind.source_code(), &source_index, encoding),
                new_text: modified[modified_range].to_owned(),
            }],
        )]
        .into_iter()
        .collect())
    }
}

pub(crate) fn parse_all(query: &DocumentQuery, fixes: Fixes) -> DocumentQuery {
    match query {
        DocumentQuery::Text {
            file_url,
            document,
            settings,
        } => {
            let mut contents = document.contents().to_owned();
            for (url, edits) in fixes {
                if url == *file_url {
                    let text_edit = edits.first().unwrap();
                    let source_index = LineIndex::from_source_text(&contents);
                    let start_offset = source_index
                        .line_start(
                            OneIndexed::from_zero_indexed(text_edit.range.start.line as usize),
                            &contents,
                        )
                        .to_usize();
                    let end_offset = source_index
                        .line_start(
                            OneIndexed::from_zero_indexed(text_edit.range.end.line as usize),
                            &contents,
                        )
                        .to_usize();
                    let mut new_contents = String::new();
                    new_contents.push_str(&contents[..start_offset]);
                    new_contents.push_str(&text_edit.new_text);
                    new_contents.push_str(&contents[end_offset..]);
                    contents = new_contents;
                }
            }
            DocumentQuery::Text {
                file_url: file_url.clone(),
                document: Arc::new(TextDocument::new(contents, document.version())),
                settings: settings.clone(),
            }
        }
        DocumentQuery::Notebook { .. } => query.clone(), // TODO
    }
}
