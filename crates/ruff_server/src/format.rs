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
) -> crate::Result<Option<String>> {
    let format_options = formatter_settings.to_format_options(source_type, document.contents());
    match format_module_source(document.contents(), format_options) {
        Ok(formatted) => {
            let formatted = formatted.into_code();
            if formatted == document.contents() {
                Ok(None)
            } else {
                Ok(Some(formatted))
            }
        }
        // Special case - syntax/parse errors are handled here instead of
        // being propagated as visible server errors.
        Err(FormatModuleError::ParseError(error)) => {
            tracing::warn!("Unable to format document: {error}");
            Ok(None)
        }
        Err(err) => Err(err.into()),
    }
}

pub(crate) fn format_range(
    document: &TextDocument,
    source_type: PySourceType,
    formatter_settings: &FormatterSettings,
    range: TextRange,
) -> crate::Result<Option<PrintedRange>> {
    let format_options = formatter_settings.to_format_options(source_type, document.contents());

    match ruff_python_formatter::format_range(document.contents(), range, format_options) {
        Ok(formatted) => {
            if formatted.as_code() == document.contents() {
                Ok(None)
            } else {
                Ok(Some(formatted))
            }
        }
        // Special case - syntax/parse errors are handled here instead of
        // being propagated as visible server errors.
        Err(FormatModuleError::ParseError(error)) => {
            tracing::warn!("Unable to format document range: {error}");
            Ok(None)
        }
        Err(err) => Err(err.into()),
    }
}
