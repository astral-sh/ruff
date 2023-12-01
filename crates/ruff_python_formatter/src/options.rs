use ruff_formatter::printer::{LineEnding, PrinterOptions, SourceMapGeneration};
use ruff_formatter::{FormatOptions, IndentStyle, IndentWidth, LineWidth};
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;
use std::path::Path;
use std::str::FromStr;

/// Resolved options for formatting one individual file. The difference to `FormatterSettings`
/// is that `FormatterSettings` stores the settings for multiple files (the entire project, a subdirectory, ..)
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct PyFormatOptions {
    /// Whether we're in a `.py` file or `.pyi` file, which have different rules.
    source_type: PySourceType,

    /// Specifies the indent style:
    /// * Either a tab
    /// * or a specific amount of spaces
    #[cfg_attr(feature = "serde", serde(default = "default_indent_style"))]
    indent_style: IndentStyle,

    /// The preferred line width at which the formatter should wrap lines.
    #[cfg_attr(feature = "serde", serde(default = "default_line_width"))]
    line_width: LineWidth,

    /// The visual width of a tab character.
    #[cfg_attr(feature = "serde", serde(default = "default_indent_width"))]
    indent_width: IndentWidth,

    line_ending: LineEnding,

    /// The preferred quote style to use (single vs double quotes).
    quote_style: QuoteStyle,

    /// Whether to expand lists or elements if they have a trailing comma such as `(a, b,)`.
    magic_trailing_comma: MagicTrailingComma,

    /// Should the formatter generate a source map that allows mapping source positions to positions
    /// in the formatted document.
    source_map_generation: SourceMapGeneration,

    /// Whether to format code snippets in docstrings or not.
    ///
    /// By default this is disabled (opt-in), but the plan is to make this
    /// enabled by default (opt-out) in the future.
    docstring_code: DocstringCode,

    /// Whether preview style formatting is enabled or not
    preview: PreviewMode,
}

fn default_line_width() -> LineWidth {
    LineWidth::try_from(88).unwrap()
}

fn default_indent_style() -> IndentStyle {
    IndentStyle::Space
}

fn default_indent_width() -> IndentWidth {
    IndentWidth::try_from(4).unwrap()
}

impl Default for PyFormatOptions {
    fn default() -> Self {
        Self {
            source_type: PySourceType::default(),
            indent_style: default_indent_style(),
            line_width: default_line_width(),
            indent_width: default_indent_width(),
            quote_style: QuoteStyle::default(),
            line_ending: LineEnding::default(),
            magic_trailing_comma: MagicTrailingComma::default(),
            source_map_generation: SourceMapGeneration::default(),
            docstring_code: DocstringCode::default(),
            preview: PreviewMode::default(),
        }
    }
}

impl PyFormatOptions {
    /// Otherwise sets the defaults. Returns none if the extension is unknown
    pub fn from_extension(path: &Path) -> Self {
        Self::from_source_type(PySourceType::from(path))
    }

    pub fn from_source_type(source_type: PySourceType) -> Self {
        Self {
            source_type,
            ..Self::default()
        }
    }

    pub fn magic_trailing_comma(&self) -> MagicTrailingComma {
        self.magic_trailing_comma
    }

    pub fn quote_style(&self) -> QuoteStyle {
        self.quote_style
    }

    pub fn source_type(&self) -> PySourceType {
        self.source_type
    }

    pub fn source_map_generation(&self) -> SourceMapGeneration {
        self.source_map_generation
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub fn docstring_code(&self) -> DocstringCode {
        self.docstring_code
    }

    pub fn preview(&self) -> PreviewMode {
        self.preview
    }

    #[must_use]
    pub fn with_indent_width(mut self, indent_width: IndentWidth) -> Self {
        self.indent_width = indent_width;
        self
    }

    #[must_use]
    pub fn with_quote_style(mut self, style: QuoteStyle) -> Self {
        self.quote_style = style;
        self
    }

    #[must_use]
    pub fn with_magic_trailing_comma(mut self, trailing_comma: MagicTrailingComma) -> Self {
        self.magic_trailing_comma = trailing_comma;
        self
    }

    #[must_use]
    pub fn with_indent_style(mut self, indent_style: IndentStyle) -> Self {
        self.indent_style = indent_style;
        self
    }

    #[must_use]
    pub fn with_line_width(mut self, line_width: LineWidth) -> Self {
        self.line_width = line_width;
        self
    }

    #[must_use]
    pub fn with_line_ending(mut self, line_ending: LineEnding) -> Self {
        self.line_ending = line_ending;
        self
    }

    #[must_use]
    pub fn with_docstring_code(mut self, docstring_code: DocstringCode) -> Self {
        self.docstring_code = docstring_code;
        self
    }

    #[must_use]
    pub fn with_preview(mut self, preview: PreviewMode) -> Self {
        self.preview = preview;
        self
    }
}

impl FormatOptions for PyFormatOptions {
    fn indent_style(&self) -> IndentStyle {
        self.indent_style
    }

    fn indent_width(&self) -> IndentWidth {
        self.indent_width
    }

    fn line_width(&self) -> LineWidth {
        self.line_width
    }

    fn as_print_options(&self) -> PrinterOptions {
        PrinterOptions {
            indent_width: self.indent_width,
            line_width: self.line_width,
            line_ending: self.line_ending,
            indent_style: self.indent_style,
            source_map_generation: self.source_map_generation,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, CacheKey)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum QuoteStyle {
    Single,
    #[default]
    Double,
}

impl QuoteStyle {
    pub const fn as_char(self) -> char {
        match self {
            QuoteStyle::Single => '\'',
            QuoteStyle::Double => '"',
        }
    }

    #[must_use]
    pub const fn invert(self) -> QuoteStyle {
        match self {
            QuoteStyle::Single => QuoteStyle::Double,
            QuoteStyle::Double => QuoteStyle::Single,
        }
    }
}

impl TryFrom<char> for QuoteStyle {
    type Error = ();

    fn try_from(value: char) -> std::result::Result<Self, Self::Error> {
        match value {
            '\'' => Ok(QuoteStyle::Single),
            '"' => Ok(QuoteStyle::Double),
            _ => Err(()),
        }
    }
}

impl FromStr for QuoteStyle {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "\"" | "double" | "Double" => Ok(Self::Double),
            "'" | "single" | "Single" => Ok(Self::Single),
            // TODO: replace this error with a diagnostic
            _ => Err("Value not supported for QuoteStyle"),
        }
    }
}

#[derive(Copy, Clone, Debug, Default, CacheKey)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
pub enum MagicTrailingComma {
    #[default]
    Respect,
    Ignore,
}

impl MagicTrailingComma {
    pub const fn is_respect(self) -> bool {
        matches!(self, Self::Respect)
    }

    pub const fn is_ignore(self) -> bool {
        matches!(self, Self::Ignore)
    }
}

impl FromStr for MagicTrailingComma {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "respect" | "Respect" => Ok(Self::Respect),
            "ignore" | "Ignore" => Ok(Self::Ignore),
            // TODO: replace this error with a diagnostic
            _ => Err("Value not supported for MagicTrailingComma"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, CacheKey)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum PreviewMode {
    #[default]
    Disabled,

    Enabled,
}

impl PreviewMode {
    pub const fn is_enabled(self) -> bool {
        matches!(self, PreviewMode::Enabled)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, CacheKey)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DocstringCode {
    #[default]
    Disabled,

    Enabled,
}

impl DocstringCode {
    pub const fn is_enabled(self) -> bool {
        matches!(self, DocstringCode::Enabled)
    }
}
