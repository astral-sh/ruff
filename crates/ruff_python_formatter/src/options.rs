use ruff_formatter::printer::{LineEnding, PrinterOptions};
use ruff_formatter::{FormatOptions, IndentStyle, LineWidth};
use ruff_python_ast::PySourceType;
use std::path::Path;
use std::str::FromStr;

#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct PyFormatOptions {
    /// Whether we're in a `.py` file or `.pyi` file, which have different rules
    source_type: PySourceType,

    /// Specifies the indent style:
    /// * Either a tab
    /// * or a specific amount of spaces
    #[cfg_attr(feature = "serde", serde(default = "default_indent_style"))]
    indent_style: IndentStyle,

    /// The preferred line width at which the formatter should wrap lines.
    #[cfg_attr(feature = "serde", serde(default = "default_line_width"))]
    line_width: LineWidth,

    /// The preferred quote style to use (single vs double quotes).
    quote_style: QuoteStyle,

    /// Whether to expand lists or elements if they have a trailing comma such as `(a, b,)`
    magic_trailing_comma: MagicTrailingComma,
}

fn default_line_width() -> LineWidth {
    LineWidth::try_from(88).unwrap()
}

fn default_indent_style() -> IndentStyle {
    IndentStyle::Space(4)
}

impl Default for PyFormatOptions {
    fn default() -> Self {
        Self {
            source_type: PySourceType::default(),
            indent_style: default_indent_style(),
            line_width: default_line_width(),
            quote_style: QuoteStyle::default(),
            magic_trailing_comma: MagicTrailingComma::default(),
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
}

impl FormatOptions for PyFormatOptions {
    fn indent_style(&self) -> IndentStyle {
        self.indent_style
    }

    fn line_width(&self) -> LineWidth {
        self.line_width
    }

    fn as_print_options(&self) -> PrinterOptions {
        PrinterOptions {
            tab_width: 4,
            print_width: self.line_width.into(),
            line_ending: LineEnding::LineFeed,
            indent_style: self.indent_style,
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
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

#[derive(Copy, Clone, Debug, Default)]
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
