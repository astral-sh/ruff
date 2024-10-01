use std::borrow::Cow;

use rustc_hash::FxHashMap;

use ruff_linter::{
    linter::{FixerResult, LinterResult},
    packaging::detect_package_root,
    settings::{flags, types::UnsafeFixes, LinterSettings},
};
use ruff_notebook::SourceValue;
use ruff_source_file::LineIndex;

use crate::{
    edit::{Replacement, ToRangeExt},
    resolve::is_document_excluded,
    session::DocumentQuery,
    PositionEncoding,
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
        if is_document_excluded(
            document_path,
            file_resolver_settings,
            Some(linter_settings),
            None,
            query.text_document_language_id(),
        ) {
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
        result: LinterResult {
            has_syntax_error, ..
        },
        ..
    } = ruff_linter::linter::lint_fix(
        &query.virtual_file_path(),
        package,
        flags::Noqa::Enabled,
        UnsafeFixes::Disabled,
        linter_settings,
        &source_kind,
        source_type,
    )?;

    if has_syntax_error {
        // If there's a syntax error, then there won't be any fixes to apply.
        return Ok(Fixes::default());
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
                    range: source_range.to_range(&source, &source_index, encoding),
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
