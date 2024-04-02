use ruff_linter::{
    linter::FixerResult,
    settings::{flags, types::UnsafeFixes, LinterSettings},
    source_kind::SourceKind,
};
use ruff_python_ast::PySourceType;
use ruff_source_file::LineIndex;
use std::path::Path;

use crate::{
    edit::{Replacement, ToRangeExt},
    PositionEncoding,
};

pub(crate) fn fix_all(
    document: &crate::edit::Document,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<Vec<lsp_types::TextEdit>> {
    let source = document.contents();

    let source_type = PySourceType::default();

    // TODO(jane): Support Jupyter Notebooks
    let source_kind = SourceKind::Python(source.to_string());

    let FixerResult { transformed, .. } = ruff_linter::linter::lint_fix(
        Path::new("<filename>"),
        None,
        flags::Noqa::Enabled,
        UnsafeFixes::Disabled,
        linter_settings,
        &source_kind,
        source_type,
    )?;

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
