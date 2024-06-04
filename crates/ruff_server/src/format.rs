use ruff_formatter::PrintedRange;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{format_module_source, FormatModuleError};
use ruff_text_size::TextRange;
use ruff_workspace::FormatterSettings;

use crate::edit::TextDocument;

pub(crate) fn format(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
) -> crate::Result<String> {
    let format_options = formatter_settings.to_format_options(source_type, document.contents());
    match format_module_source(document.contents(), format_options) {
        Ok(formatted) => Ok(formatted.into_code()),
        // Special case - syntax/parse errors should be handled here instead of
        // being propagated to become visible server errors.
        Err(FormatModuleError::ParseError(error)) => {
            tracing::warn!("Unable to format document: {error}");
            Ok(document.contents().to_string())
        }
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn format_range(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
) -> crate::Result<PrintedRange> {
    let format_options = formatter_settings.to_format_options(source_type, document.contents());

    match ruff_python_formatter::format_range(document.contents(), range, format_options) {
        Ok(formatted) => Ok(formatted),
        // Special case - syntax/parse errors should be handled here instead of
        // being propagated to become visible server errors.
        Err(FormatModuleError::ParseError(error)) => {
            tracing::warn!("Unable to format document range: {error}");
            Ok(PrintedRange::new(document.contents().to_string(), range))
        }
        Err(err) => Err(err.into()),
    }
}
