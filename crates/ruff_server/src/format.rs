use ruff_formatter::PrintedRange;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::format_module_source;
use ruff_text_size::TextRange;
use ruff_workspace::FormatterSettings;

use crate::edit::TextDocument;

pub(crate) fn format(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
) -> crate::Result<String> {
    let format_options = formatter_settings.to_format_options(source_type, document.contents());
    let formatted = format_module_source(document.contents(), format_options)?;
    Ok(formatted.into_code())
}

pub(crate) fn format_range(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
) -> crate::Result<PrintedRange> {
    let format_options = formatter_settings.to_format_options(source_type, document.contents());

    Ok(ruff_python_formatter::format_range(
        document.contents(),
        range,
        format_options,
    )?)
}
