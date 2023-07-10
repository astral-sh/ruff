use ruff_formatter::printer::{LineEnding, PrinterOptions};
use ruff_formatter::{FormatOptions, IndentStyle, LineWidth};

#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct PyFormatOptions {
    /// Specifies the indent style:
    /// * Either a tab
    /// * or a specific amount of spaces
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

impl PyFormatOptions {
    pub fn magic_trailing_comma(&self) -> MagicTrailingComma {
        self.magic_trailing_comma
    }

    pub fn quote_style(&self) -> QuoteStyle {
        self.quote_style
    }

    pub fn with_quote_style(&mut self, style: QuoteStyle) -> &mut Self {
        self.quote_style = style;
        self
    }

    pub fn with_magic_trailing_comma(&mut self, trailing_comma: MagicTrailingComma) -> &mut Self {
        self.magic_trailing_comma = trailing_comma;
        self
    }

    pub fn with_indent_style(&mut self, indent_style: IndentStyle) -> &mut Self {
        self.indent_style = indent_style;
        self
    }

    pub fn with_line_width(&mut self, line_width: LineWidth) -> &mut Self {
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

impl Default for PyFormatOptions {
    fn default() -> Self {
        Self {
            indent_style: IndentStyle::Space(4),
            line_width: LineWidth::try_from(88).unwrap(),
            quote_style: QuoteStyle::default(),
            magic_trailing_comma: MagicTrailingComma::default(),
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
