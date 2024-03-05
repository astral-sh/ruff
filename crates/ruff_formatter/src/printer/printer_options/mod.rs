use crate::{FormatOptions, IndentStyle, IndentWidth, LineWidth};

/// Options that affect how the [`crate::Printer`] prints the format tokens
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct PrinterOptions {
    /// Width of a single tab character (does it equal 2, 4, ... spaces?)
    pub indent_width: IndentWidth,

    /// Whether the printer should use tabs or spaces to indent code.
    pub indent_style: IndentStyle,

    /// What's the max width of a line. Defaults to 80
    pub line_width: LineWidth,

    /// The type of line ending to apply to the printed input
    pub line_ending: LineEnding,
}

impl<'a, O> From<&'a O> for PrinterOptions
where
    O: FormatOptions,
{
    fn from(options: &'a O) -> Self {
        PrinterOptions::default()
            .with_indent(options.indent_style())
            .with_line_width(options.line_width())
    }
}

impl PrinterOptions {
    #[must_use]
    pub fn with_line_width(mut self, width: LineWidth) -> Self {
        self.line_width = width;
        self
    }

    #[must_use]
    pub fn with_indent(mut self, style: IndentStyle) -> Self {
        self.indent_style = style;

        self
    }

    #[must_use]
    pub fn with_tab_width(mut self, width: IndentWidth) -> Self {
        self.indent_width = width;

        self
    }

    pub(crate) fn indent_style(&self) -> IndentStyle {
        self.indent_style
    }

    /// Width of an indent in characters.
    pub(super) const fn indent_width(&self) -> u32 {
        self.indent_width.value()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct PrintWidth(u16);

impl PrintWidth {
    pub fn new(width: u16) -> Self {
        Self(width)
    }
}

impl Default for PrintWidth {
    fn default() -> Self {
        LineWidth::default().into()
    }
}

impl From<LineWidth> for PrintWidth {
    fn from(width: LineWidth) -> Self {
        Self(u16::from(width))
    }
}

impl From<PrintWidth> for u32 {
    fn from(width: PrintWidth) -> Self {
        u32::from(width.0)
    }
}

impl From<PrintWidth> for u16 {
    fn from(width: PrintWidth) -> Self {
        width.0
    }
}

/// Configures whether the formatter and printer generate a source map that allows mapping
/// positions in the source document to positions in the formatted code.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SourceMapGeneration {
    /// The formatter generates no source map.
    #[default]
    Disabled,

    /// The formatter generates a source map that allows mapping positions in the source document
    /// to positions in the formatted document. The ability to map positions is useful for range formatting
    /// or when trying to identify where to move the cursor so that it matches its position in the source document.
    Enabled,
}

impl SourceMapGeneration {
    pub const fn is_enabled(self) -> bool {
        matches!(self, SourceMapGeneration::Enabled)
    }

    pub const fn is_disabled(self) -> bool {
        matches!(self, SourceMapGeneration::Disabled)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LineEnding {
    ///  Line Feed only (\n), common on Linux and macOS as well as inside git repos
    #[default]
    LineFeed,

    /// Carriage Return + Line Feed characters (\r\n), common on Windows
    CarriageReturnLineFeed,

    /// Carriage Return character only (\r), used very rarely
    CarriageReturn,
}

impl LineEnding {
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            LineEnding::LineFeed => "\n",
            LineEnding::CarriageReturnLineFeed => "\r\n",
            LineEnding::CarriageReturn => "\r",
        }
    }
}
