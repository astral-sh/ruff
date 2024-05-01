use ruff_linter::{
    linter::{FixerResult, LinterResult},
    packaging::detect_package_root,
    settings::{flags, types::UnsafeFixes, LinterSettings},
    source_kind::SourceKind,
};
use ruff_python_ast::PySourceType;
use ruff_source_file::LineIndex;
use std::borrow::Cow;

use crate::{
    edit::{Replacement, ToRangeExt},
    PositionEncoding,
};

pub(crate) fn fix_all(
    document: &crate::edit::Document,
    document_url: &lsp_types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<Vec<lsp_types::TextEdit>> {
    let source = document.contents();

    let document_path = document_url
        .to_file_path()
        .expect("document URL should be a valid file path");

    let package = detect_package_root(
        document_path
            .parent()
            .expect("a path to a document should have a parent path"),
        &linter_settings.namespace_packages,
    );

    let source_type = PySourceType::default();

    // TODO(jane): Support Jupyter Notebooks
    let source_kind = SourceKind::Python(source.to_string());

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
        &document_path,
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
        return Ok(vec![]);
    }

    let modified = transformed.source_code();

    let modified_index = LineIndex::from_source_text(modified);

    let source_index = document.index();

    let Replacement {
        source_range,
        modified_range,
    } = Replacement::between(
        source,
        source_index.line_starts(),
        modified,
        modified_index.line_starts(),
    );

    Ok(vec![lsp_types::TextEdit {
        range: source_range.to_range(source, source_index, encoding),
        new_text: modified[modified_range].to_owned(),
    }])
}
