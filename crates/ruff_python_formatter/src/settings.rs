use std::path::PathBuf;

use ruff_formatter::{FormatOptions, IndentStyle, LineWidth};
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;

use crate::{MagicTrailingComma, PreviewMode, PyFormatOptions, QuoteStyle};

#[derive(CacheKey, Clone, Debug)]
pub struct FormatterSettings {
    /// The files that are excluded from formatting (but may be linted).
    pub exclude: Vec<PathBuf>,

    pub preview: PreviewMode,

    pub line_width: LineWidth,

    pub indent_style: IndentStyle,

    pub quote_style: QuoteStyle,

    pub magic_trailing_comma: MagicTrailingComma,
}

impl FormatterSettings {
    pub fn to_format_options(&self, source_type: PySourceType) -> PyFormatOptions {
        PyFormatOptions::from_source_type(source_type)
            .with_indent_style(self.indent_style)
            .with_quote_style(self.quote_style)
            .with_magic_trailing_comma(self.magic_trailing_comma)
            .with_preview(self.preview)
            .with_line_width(self.line_width)
    }
}

impl Default for FormatterSettings {
    fn default() -> Self {
        let default_options = PyFormatOptions::default();

        Self {
            exclude: Vec::default(),
            preview: PreviewMode::Disabled,
            line_width: default_options.line_width(),
            indent_style: default_options.indent_style(),
            quote_style: default_options.quote_style(),
            magic_trailing_comma: default_options.magic_trailing_comma(),
        }
    }
}
