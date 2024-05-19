use std::fmt;
use std::path::Path;
use std::str::FromStr;

use ruff_formatter::printer::{LineEnding, PrinterOptions, SourceMapGeneration};
use ruff_formatter::{FormatOptions, IndentStyle, IndentWidth, LineWidth};
use ruff_macros::CacheKey;
use ruff_python_ast::PySourceType;

/// Resolved options for formatting one individual file. The difference to `FormatterSettings`
/// is that `FormatterSettings` stores the settings for multiple files (the entire project, a subdirectory, ..)
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, deny_unknown_fields)
)]
pub struct PyFormatOptions {
    /// Whether we're in a `.py` file or `.pyi` file, which have different rules.
    source_type: PySourceType,

    /// The (minimum) Python version used to run the formatted code. This is used
    /// to determine the supported Python syntax.
    target_version: PythonVersion,

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

    /// The preferred line width at which the formatter should wrap lines in
    /// docstring code examples. This only has an impact when `docstring_code`
    /// is enabled.
    docstring_code_line_width: DocstringCodeLineWidth,

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
            target_version: PythonVersion::default(),
            indent_style: default_indent_style(),
            line_width: default_line_width(),
            indent_width: default_indent_width(),
            quote_style: QuoteStyle::default(),
            line_ending: LineEnding::default(),
            magic_trailing_comma: MagicTrailingComma::default(),
            source_map_generation: SourceMapGeneration::default(),
            docstring_code: DocstringCode::default(),
            docstring_code_line_width: DocstringCodeLineWidth::default(),
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

    pub const fn target_version(&self) -> PythonVersion {
        self.target_version
    }

    pub const fn magic_trailing_comma(&self) -> MagicTrailingComma {
        self.magic_trailing_comma
    }

    pub const fn quote_style(&self) -> QuoteStyle {
        self.quote_style
    }

    pub const fn source_type(&self) -> PySourceType {
        self.source_type
    }

    pub const fn source_map_generation(&self) -> SourceMapGeneration {
        self.source_map_generation
    }

    pub const fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub const fn docstring_code(&self) -> DocstringCode {
        self.docstring_code
    }

    pub const fn docstring_code_line_width(&self) -> DocstringCodeLineWidth {
        self.docstring_code_line_width
    }

    pub const fn preview(&self) -> PreviewMode {
        self.preview
    }

    #[must_use]
    pub fn with_target_version(mut self, target_version: PythonVersion) -> Self {
        self.target_version = target_version;
        self
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
    pub fn with_docstring_code_line_width(mut self, line_width: DocstringCodeLineWidth) -> Self {
        self.docstring_code_line_width = line_width;
        self
    }

    #[must_use]
    pub fn with_preview(mut self, preview: PreviewMode) -> Self {
        self.preview = preview;
        self
    }

    #[must_use]
    pub fn with_source_map_generation(mut self, source_map: SourceMapGeneration) -> Self {
        self.source_map_generation = source_map;
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
    Preserve,
}

impl QuoteStyle {
    pub const fn is_preserve(self) -> bool {
        matches!(self, QuoteStyle::Preserve)
    }
}

impl fmt::Display for QuoteStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Single => write!(f, "single"),
            Self::Double => write!(f, "double"),
            Self::Preserve => write!(f, "preserve"),
        }
    }
}

impl FromStr for QuoteStyle {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "\"" | "double" | "Double" => Ok(Self::Double),
            "'" | "single" | "Single" => Ok(Self::Single),
            "preserve" | "Preserve" => Ok(Self::Preserve),
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

impl fmt::Display for MagicTrailingComma {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Respect => write!(f, "respect"),
            Self::Ignore => write!(f, "ignore"),
        }
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

impl fmt::Display for PreviewMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Enabled => write!(f, "enabled"),
        }
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

impl fmt::Display for DocstringCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Enabled => write!(f, "enabled"),
        }
    }
}

#[derive(Copy, Clone, Default, Eq, PartialEq, CacheKey)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum DocstringCodeLineWidth {
    /// Wrap docstring code examples at a fixed line width.
    Fixed(LineWidth),

    /// Respect the line length limit setting for the surrounding Python code.
    #[default]
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "deserialize_docstring_code_line_width_dynamic")
    )]
    #[cfg_attr(feature = "schemars", schemars(with = "DynamicSchema"))]
    Dynamic,
}

/// A dummy type that is used to generate a schema for `DocstringCodeLineWidth::Dynamic`.
#[cfg(feature = "schemars")]
struct DynamicSchema;

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for DynamicSchema {
    fn schema_name() -> String {
        "Dynamic".to_string()
    }

    fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            const_value: Some("dynamic".to_string().into()),
            ..Default::default()
        }
        .into()
    }
}

impl fmt::Debug for DocstringCodeLineWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DocstringCodeLineWidth::Fixed(v) => v.value().fmt(f),
            DocstringCodeLineWidth::Dynamic => "dynamic".fmt(f),
        }
    }
}

impl fmt::Display for DocstringCodeLineWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(width) => width.fmt(f),
            Self::Dynamic => write!(f, "dynamic"),
        }
    }
}

/// Responsible for deserializing the `DocstringCodeLineWidth::Dynamic`
/// variant.
fn deserialize_docstring_code_line_width_dynamic<'de, D>(d: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::{de::Error, Deserialize};

    let value = String::deserialize(d)?;
    match &*value {
        "dynamic" => Ok(()),
        s => Err(D::Error::invalid_value(
            serde::de::Unexpected::Str(s),
            &"dynamic",
        )),
    }
}

#[derive(CacheKey, Clone, Copy, Debug, PartialOrd, Ord, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum PythonVersion {
    Py37,
    // Make sure to also change the default for `ruff_linter::settings::types::PythonVersion`
    // when changing the default here.
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl PythonVersion {
    /// Return `true` if the current version supports [PEP 701].
    ///
    /// [PEP 701]: https://peps.python.org/pep-0701/
    pub fn supports_pep_701(self) -> bool {
        self >= Self::Py312
    }
}
