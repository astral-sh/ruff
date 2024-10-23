//! Infrastructure for code formatting
//!
//! This module defines [`FormatElement`], an IR to format code documents and provides a means to print
//! such a document to a string. Objects that know how to format themselves implement the [Format] trait.
//!
//! ## Formatting Traits
//!
//! * [`Format`]: Implemented by objects that can be formatted.
//! * [`FormatRule`]: Rule that knows how to format an object of another type. Useful in the situation where
//!    it's necessary to implement [Format] on an object from another crate. This module defines the
//!    [`FormatRefWithRule`] and [`FormatOwnedWithRule`] structs to pass an item with its corresponding rule.
//! * [`FormatWithRule`] implemented by objects that know how to format another type. Useful for implementing
//!    some reusable formatting logic inside of this module if the type itself doesn't implement [Format]
//!
//! ## Formatting Macros
//!
//! This crate defines two macros to construct the IR. These are inspired by Rust's `fmt` macros
//! * [`format!`]: Formats a formattable object
//! * [`format_args!`]: Concatenates a sequence of Format objects.
//! * [`write!`]: Writes a sequence of formattable objects into an output buffer.

mod arguments;
mod buffer;
mod builders;
pub mod diagnostics;
pub mod format_element;
mod format_extensions;
pub mod formatter;
pub mod group_id;
pub mod macros;
pub mod prelude;
pub mod printer;
mod source_code;

use crate::formatter::Formatter;
use crate::group_id::UniqueGroupIdBuilder;
use crate::prelude::TagKind;
use std::fmt;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::num::{NonZeroU16, NonZeroU8, TryFromIntError};

use crate::format_element::document::Document;
use crate::printer::{Printer, PrinterOptions};
pub use arguments::{Argument, Arguments};
pub use buffer::{
    Buffer, BufferExtensions, BufferSnapshot, Inspect, RemoveSoftLinesBuffer, VecBuffer,
};
pub use builders::BestFitting;
pub use source_code::{SourceCode, SourceCodeSlice};

pub use crate::diagnostics::{ActualStart, FormatError, InvalidDocumentError, PrintError};
pub use format_element::{normalize_newlines, FormatElement, LINE_TERMINATORS};
pub use group_id::GroupId;
use ruff_macros::CacheKey;
use ruff_text_size::{TextLen, TextRange, TextSize};

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash, CacheKey)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "kebab-case")
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Default)]
pub enum IndentStyle {
    /// Use tabs to indent code.
    #[default]
    Tab,
    /// Use [`IndentWidth`] spaces to indent code.
    Space,
}

impl IndentStyle {
    /// Returns `true` if this is an [`IndentStyle::Tab`].
    pub const fn is_tab(&self) -> bool {
        matches!(self, IndentStyle::Tab)
    }

    /// Returns `true` if this is an [`IndentStyle::Space`].
    pub const fn is_space(&self) -> bool {
        matches!(self, IndentStyle::Space)
    }
}

impl std::fmt::Display for IndentStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndentStyle::Tab => std::write!(f, "tab"),
            IndentStyle::Space => std::write!(f, "space"),
        }
    }
}

/// The visual width of a indentation.
///
/// Determines the visual width of a tab character (`\t`) and the number of
/// spaces per indent when using [`IndentStyle::Space`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, CacheKey)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IndentWidth(NonZeroU8);

impl IndentWidth {
    /// Return the numeric value for this [`LineWidth`]
    pub const fn value(&self) -> u32 {
        self.0.get() as u32
    }
}

impl Default for IndentWidth {
    fn default() -> Self {
        Self(NonZeroU8::new(2).unwrap())
    }
}

impl Display for IndentWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl TryFrom<u8> for IndentWidth {
    type Error = TryFromIntError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        NonZeroU8::try_from(value).map(Self)
    }
}

impl From<NonZeroU8> for IndentWidth {
    fn from(value: NonZeroU8) -> Self {
        Self(value)
    }
}

/// The maximum visual width to which the formatter should try to limit a line.
#[derive(Clone, Copy, Debug, Eq, PartialEq, CacheKey)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LineWidth(NonZeroU16);

impl LineWidth {
    /// Return the numeric value for this [`LineWidth`]
    pub const fn value(&self) -> u16 {
        self.0.get()
    }
}

impl Default for LineWidth {
    fn default() -> Self {
        Self(NonZeroU16::new(80).unwrap())
    }
}

impl Display for LineWidth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl TryFrom<u16> for LineWidth {
    type Error = TryFromIntError;

    fn try_from(value: u16) -> Result<LineWidth, Self::Error> {
        NonZeroU16::try_from(value).map(LineWidth)
    }
}

impl From<LineWidth> for u16 {
    fn from(value: LineWidth) -> Self {
        value.0.get()
    }
}

impl From<LineWidth> for u32 {
    fn from(value: LineWidth) -> Self {
        u32::from(value.0.get())
    }
}

impl From<NonZeroU16> for LineWidth {
    fn from(value: NonZeroU16) -> Self {
        Self(value)
    }
}

/// Context object storing data relevant when formatting an object.
pub trait FormatContext {
    type Options: FormatOptions;

    /// Returns the formatting options
    fn options(&self) -> &Self::Options;

    /// Returns the source code from the document that gets formatted.
    fn source_code(&self) -> SourceCode;
}

/// Options customizing how the source code should be formatted.
pub trait FormatOptions {
    /// The indent style.
    fn indent_style(&self) -> IndentStyle;

    /// The visual width of an indent
    fn indent_width(&self) -> IndentWidth;

    /// What's the max width of a line. Defaults to 80.
    fn line_width(&self) -> LineWidth;

    /// Derives the print options from the these format options
    fn as_print_options(&self) -> PrinterOptions;
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct SimpleFormatContext {
    options: SimpleFormatOptions,
    source_code: String,
}

impl SimpleFormatContext {
    pub fn new(options: SimpleFormatOptions) -> Self {
        Self {
            options,
            source_code: String::new(),
        }
    }

    #[must_use]
    pub fn with_source_code(mut self, code: &str) -> Self {
        self.source_code = String::from(code);
        self
    }
}

impl FormatContext for SimpleFormatContext {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }

    fn source_code(&self) -> SourceCode {
        SourceCode::new(&self.source_code)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone)]
pub struct SimpleFormatOptions {
    pub indent_style: IndentStyle,
    pub indent_width: IndentWidth,
    pub line_width: LineWidth,
}

impl FormatOptions for SimpleFormatOptions {
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
            line_width: self.line_width,
            indent_style: self.indent_style,
            indent_width: self.indent_width,
            ..PrinterOptions::default()
        }
    }
}

/// Lightweight sourcemap marker between source and output tokens
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct SourceMarker {
    /// Position of the marker in the original source
    pub source: TextSize,
    /// Position of the marker in the output code
    pub dest: TextSize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Formatted<Context> {
    document: Document,
    context: Context,
}

impl<Context> Formatted<Context> {
    pub fn new(document: Document, context: Context) -> Self {
        Self { document, context }
    }

    /// Returns the context used during formatting.
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Returns the formatted document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Consumes `self` and returns the formatted document.
    pub fn into_document(self) -> Document {
        self.document
    }
}

impl<Context> Formatted<Context>
where
    Context: FormatContext,
{
    pub fn print(&self) -> PrintResult<Printed> {
        let printer = self.create_printer();
        printer.print(&self.document)
    }

    pub fn print_with_indent(&self, indent: u16) -> PrintResult<Printed> {
        let printer = self.create_printer();
        printer.print_with_indent(&self.document, indent)
    }

    fn create_printer(&self) -> Printer {
        let source_code = self.context.source_code();
        let print_options = self.context.options().as_print_options();

        Printer::new(source_code, print_options)
    }
}

impl<Context> Display for Formatted<Context>
where
    Context: FormatContext,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.document.display(self.context.source_code()), f)
    }
}

pub type PrintResult<T> = Result<T, PrintError>;

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Printed {
    code: String,
    range: Option<TextRange>,
    sourcemap: Vec<SourceMarker>,
    verbatim_ranges: Vec<TextRange>,
}

impl Printed {
    pub fn new(
        code: String,
        range: Option<TextRange>,
        sourcemap: Vec<SourceMarker>,
        verbatim_source: Vec<TextRange>,
    ) -> Self {
        Self {
            code,
            range,
            sourcemap,
            verbatim_ranges: verbatim_source,
        }
    }

    /// Construct an empty formatter result
    pub fn new_empty() -> Self {
        Self {
            code: String::new(),
            range: None,
            sourcemap: Vec::new(),
            verbatim_ranges: Vec::new(),
        }
    }

    /// Range of the input source file covered by this formatted code,
    /// or None if the entire file is covered in this instance
    pub fn range(&self) -> Option<TextRange> {
        self.range
    }

    /// Returns a list of [`SourceMarker`] mapping byte positions
    /// in the output string to the input source code.
    /// It's not guaranteed that the markers are sorted by source position.
    pub fn sourcemap(&self) -> &[SourceMarker] {
        &self.sourcemap
    }

    /// Returns a list of [`SourceMarker`] mapping byte positions
    /// in the output string to the input source code, consuming the result
    pub fn into_sourcemap(self) -> Vec<SourceMarker> {
        self.sourcemap
    }

    /// Takes the list of [`SourceMarker`] mapping byte positions in the output string
    /// to the input source code.
    pub fn take_sourcemap(&mut self) -> Vec<SourceMarker> {
        std::mem::take(&mut self.sourcemap)
    }

    /// Access the resulting code, borrowing the result
    pub fn as_code(&self) -> &str {
        &self.code
    }

    /// Access the resulting code, consuming the result
    pub fn into_code(self) -> String {
        self.code
    }

    /// The text in the formatted code that has been formatted as verbatim.
    pub fn verbatim(&self) -> impl Iterator<Item = (TextRange, &str)> {
        self.verbatim_ranges
            .iter()
            .map(|range| (*range, &self.code[*range]))
    }

    /// Ranges of the formatted code that have been formatted as verbatim.
    pub fn verbatim_ranges(&self) -> &[TextRange] {
        &self.verbatim_ranges
    }

    /// Takes the ranges of nodes that have been formatted as verbatim, replacing them with an empty list.
    pub fn take_verbatim_ranges(&mut self) -> Vec<TextRange> {
        std::mem::take(&mut self.verbatim_ranges)
    }

    /// Slices the formatted code to the sub-slices that covers the passed `source_range` in `source`.
    ///
    /// The implementation uses the source map generated during formatting to find the closest range
    /// in the formatted document that covers `source_range` or more. The returned slice
    /// matches the `source_range` exactly (except indent, see below) if the formatter emits [`FormatElement::SourcePosition`] for
    /// the range's offsets.
    ///
    /// ## Indentation
    /// The indentation before `source_range.start` is replaced with the indentation returned by the formatter
    /// to fix up incorrectly intended code.
    ///
    /// Returns the entire document if the source map is empty.
    ///
    /// # Panics
    /// If `source_range` points to offsets that are not in the bounds of `source`.
    #[must_use]
    pub fn slice_range(self, source_range: TextRange, source: &str) -> PrintedRange {
        let mut start_marker: Option<SourceMarker> = None;
        let mut end_marker: Option<SourceMarker> = None;

        // Note: The printer can generate multiple source map entries for the same source position.
        // For example if you have:
        // * token("a + b")
        // * `source_position(276)`
        // * `token(")")`
        // * `source_position(276)`
        // *  `hard_line_break`
        // The printer uses the source position 276 for both the tokens `)` and the `\n` because
        // there were multiple `source_position` entries in the IR with the same offset.
        // This can happen if multiple nodes start or end at the same position. A common example
        // for this are expressions and expression statement that always end at the same offset.
        //
        // Warning: Source markers are often emitted sorted by their source position but it's not guaranteed
        // and depends on the emitted `IR`.
        // They are only guaranteed to be sorted in increasing order by their destination position.
        for marker in self.sourcemap {
            // Take the closest start marker, but skip over start_markers that have the same start.
            if marker.source <= source_range.start()
                && !start_marker.is_some_and(|existing| existing.source >= marker.source)
            {
                start_marker = Some(marker);
            }

            if marker.source >= source_range.end()
                && !end_marker.is_some_and(|existing| existing.source <= marker.source)
            {
                end_marker = Some(marker);
            }
        }

        let (source_start, formatted_start) = start_marker
            .map(|marker| (marker.source, marker.dest))
            .unwrap_or_default();

        let (source_end, formatted_end) = end_marker
            .map_or((source.text_len(), self.code.text_len()), |marker| {
                (marker.source, marker.dest)
            });

        let source_range = TextRange::new(source_start, source_end);
        let formatted_range = TextRange::new(formatted_start, formatted_end);

        // Extend both ranges to include the indentation
        let source_range = extend_range_to_include_indent(source_range, source);
        let formatted_range = extend_range_to_include_indent(formatted_range, &self.code);

        PrintedRange {
            code: self.code[formatted_range].to_string(),
            source_range,
        }
    }
}

/// Extends `range` backwards (by reducing `range.start`) to include any directly preceding whitespace (`\t` or ` `).
///
/// # Panics
/// If `range.start` is out of `source`'s bounds.
fn extend_range_to_include_indent(range: TextRange, source: &str) -> TextRange {
    let whitespace_len: TextSize = source[..usize::from(range.start())]
        .chars()
        .rev()
        .take_while(|c| matches!(c, ' ' | '\t'))
        .map(TextLen::text_len)
        .sum();

    TextRange::new(range.start() - whitespace_len, range.end())
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PrintedRange {
    code: String,
    source_range: TextRange,
}

impl PrintedRange {
    pub fn new(code: String, source_range: TextRange) -> Self {
        Self { code, source_range }
    }

    pub fn empty() -> Self {
        Self {
            code: String::new(),
            source_range: TextRange::default(),
        }
    }

    /// The formatted code.
    pub fn as_code(&self) -> &str {
        &self.code
    }

    pub fn into_code(self) -> String {
        self.code
    }

    /// The range the formatted code corresponds to in the source document.
    pub fn source_range(&self) -> TextRange {
        self.source_range
    }
}

/// Public return type of the formatter
pub type FormatResult<F> = Result<F, FormatError>;

/// Formatting trait for types that can create a formatted representation. The `ruff_formatter` equivalent
/// to [`std::fmt::Display`].
///
/// ## Example
/// Implementing `Format` for a custom struct
///
/// ```
/// use ruff_formatter::{format, write, IndentStyle};
/// use ruff_formatter::prelude::*;
/// use ruff_text_size::TextSize;
///
/// struct Paragraph(String);
///
/// impl Format<SimpleFormatContext> for Paragraph {
///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
///         write!(f, [
///             text(&self.0),
///             hard_line_break(),
///         ])
///     }
/// }
///
/// # fn main() -> FormatResult<()> {
/// let paragraph = Paragraph(String::from("test"));
/// let formatted = format!(SimpleFormatContext::default(), [paragraph])?;
///
/// assert_eq!("test\n", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
pub trait Format<Context> {
    /// Formats the object using the given formatter.
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()>;
}

impl<T, Context> Format<Context> for &T
where
    T: ?Sized + Format<Context>,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        Format::fmt(&**self, f)
    }
}

impl<T, Context> Format<Context> for &mut T
where
    T: ?Sized + Format<Context>,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        Format::fmt(&**self, f)
    }
}

impl<T, Context> Format<Context> for Option<T>
where
    T: Format<Context>,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        match self {
            Some(value) => value.fmt(f),
            None => Ok(()),
        }
    }
}

impl<Context> Format<Context> for () {
    #[inline]
    fn fmt(&self, _: &mut Formatter<Context>) -> FormatResult<()> {
        // Intentionally left empty
        Ok(())
    }
}

/// Rule that knows how to format an object of type `T`.
///
/// Implementing [Format] on the object itself is preferred over implementing [`FormatRule`] but
/// this isn't possible inside of a dependent crate for external type.
///
/// For example, the `ruff_js_formatter` crate isn't able to implement [Format] on `JsIfStatement`
/// because both the [Format] trait and `JsIfStatement` are external types (Rust's orphan rule).
///
/// That's why the `ruff_js_formatter` crate must define a new-type that implements the formatting
/// of `JsIfStatement`.
pub trait FormatRule<T, C> {
    fn fmt(&self, item: &T, f: &mut Formatter<C>) -> FormatResult<()>;
}

/// Rule that supports customizing how it formats an object of type `T`.
pub trait FormatRuleWithOptions<T, C>: FormatRule<T, C> {
    type Options;

    /// Returns a new rule that uses the given options to format an object.
    #[must_use]
    fn with_options(self, options: Self::Options) -> Self;
}

/// Trait for an object that formats an object with a specified rule.
///
/// Gives access to the underlying item.
///
/// Useful in situation where a type itself doesn't implement [Format] (e.g. because of Rust's orphan rule)
/// but you want to implement some common formatting logic.
///
/// ## Examples
///
/// This can be useful if you want to format a `SyntaxNode` inside `ruff_formatter`.. `SyntaxNode` doesn't implement [Format]
/// itself but the language specific crate implements `AsFormat` and `IntoFormat` for it and the returned [Format]
/// implement [`FormatWithRule`].
///
/// ```ignore
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{format, Formatted, FormatWithRule};
/// use ruff_rowan::{Language, SyntaxNode};
/// fn format_node<L: Language, F: FormatWithRule<SimpleFormatContext, Item=SyntaxNode<L>>>(node: F) -> FormatResult<Formatted<SimpleFormatContext>> {
///     let formatted = format!(SimpleFormatContext::default(), [node]);
///     let syntax = node.item();
///     // Do something with syntax
///     formatted;
/// }
/// ```
pub trait FormatWithRule<Context>: Format<Context> {
    type Item;

    /// Returns the associated item
    fn item(&self) -> &Self::Item;
}

/// Formats the referenced `item` with the specified rule.
#[derive(Debug, Copy, Clone)]
pub struct FormatRefWithRule<'a, T, R, C>
where
    R: FormatRule<T, C>,
{
    item: &'a T,
    rule: R,
    context: PhantomData<C>,
}

impl<'a, T, R, C> FormatRefWithRule<'a, T, R, C>
where
    R: FormatRule<T, C>,
{
    pub fn new(item: &'a T, rule: R) -> Self {
        Self {
            item,
            rule,
            context: PhantomData,
        }
    }

    pub fn rule(&self) -> &R {
        &self.rule
    }
}

impl<T, R, O, C> FormatRefWithRule<'_, T, R, C>
where
    R: FormatRuleWithOptions<T, C, Options = O>,
{
    #[must_use]
    pub fn with_options(mut self, options: O) -> Self {
        self.rule = self.rule.with_options(options);
        self
    }
}

impl<T, R, C> FormatWithRule<C> for FormatRefWithRule<'_, T, R, C>
where
    R: FormatRule<T, C>,
{
    type Item = T;

    fn item(&self) -> &Self::Item {
        self.item
    }
}

impl<T, R, C> Format<C> for FormatRefWithRule<'_, T, R, C>
where
    R: FormatRule<T, C>,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        self.rule.fmt(self.item, f)
    }
}

/// Formats the `item` with the specified rule.
#[derive(Debug, Clone)]
pub struct FormatOwnedWithRule<T, R, C>
where
    R: FormatRule<T, C>,
{
    item: T,
    rule: R,
    context: PhantomData<C>,
}

impl<T, R, C> FormatOwnedWithRule<T, R, C>
where
    R: FormatRule<T, C>,
{
    pub fn new(item: T, rule: R) -> Self {
        Self {
            item,
            rule,
            context: PhantomData,
        }
    }

    #[must_use]
    pub fn with_item(mut self, item: T) -> Self {
        self.item = item;
        self
    }
}

impl<T, R, C> Format<C> for FormatOwnedWithRule<T, R, C>
where
    R: FormatRule<T, C>,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter<C>) -> FormatResult<()> {
        self.rule.fmt(&self.item, f)
    }
}

impl<T, R, O, C> FormatOwnedWithRule<T, R, C>
where
    R: FormatRuleWithOptions<T, C, Options = O>,
{
    #[must_use]
    pub fn with_options(mut self, options: O) -> Self {
        self.rule = self.rule.with_options(options);
        self
    }
}

impl<T, R, C> FormatWithRule<C> for FormatOwnedWithRule<T, R, C>
where
    R: FormatRule<T, C>,
{
    type Item = T;

    fn item(&self) -> &Self::Item {
        &self.item
    }
}

/// The `write` function takes a target buffer and an `Arguments` struct that can be precompiled with the `format_args!` macro.
///
/// The arguments will be formatted in-order into the output buffer provided.
///
/// # Examples
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{VecBuffer, format_args, FormatState, write, Formatted};
///
/// # fn main() -> FormatResult<()> {
/// let mut state = FormatState::new(SimpleFormatContext::default());
/// let mut buffer = VecBuffer::new(&mut state);
///
/// write!(&mut buffer, [format_args!(token("Hello World"))])?;
///
/// let formatted = Formatted::new(Document::from(buffer.into_vec()), SimpleFormatContext::default());
///
/// assert_eq!("Hello World", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
/// Please note that using [`write!`] might be preferable. Example:
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{VecBuffer, format_args, FormatState, write, Formatted};
///
/// # fn main() -> FormatResult<()> {
/// let mut state = FormatState::new(SimpleFormatContext::default());
/// let mut buffer = VecBuffer::new(&mut state);
///
/// write!(&mut buffer, [token("Hello World")])?;
///
/// let formatted = Formatted::new(Document::from(buffer.into_vec()), SimpleFormatContext::default());
///
/// assert_eq!("Hello World", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn write<Context>(
    output: &mut dyn Buffer<Context = Context>,
    args: Arguments<Context>,
) -> FormatResult<()> {
    let mut f = Formatter::new(output);

    f.write_fmt(args)
}

/// The `format` function takes an [`Arguments`] struct and returns the resulting formatting IR.
///
/// The [`Arguments`] instance can be created with the [`format_args!`].
///
/// # Examples
///
/// Basic usage:
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{format, format_args};
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [&format_args!(token("test"))])?;
/// assert_eq!("test", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
/// Please note that using [`format!`] might be preferable. Example:
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{format};
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [token("test")])?;
/// assert_eq!("test", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
pub fn format<Context>(
    context: Context,
    arguments: Arguments<Context>,
) -> FormatResult<Formatted<Context>>
where
    Context: FormatContext,
{
    let source_length = context.source_code().as_str().len();
    // Use a simple heuristic to guess the number of expected format elements.
    // See [#6612](https://github.com/astral-sh/ruff/pull/6612) for more details on how the formula was determined. Changes to our formatter, or supporting
    // more languages may require fine tuning the formula.
    let estimated_buffer_size = source_length / 2;
    let mut state = FormatState::new(context);
    let mut buffer = VecBuffer::with_capacity(estimated_buffer_size, &mut state);

    buffer.write_fmt(arguments)?;

    let mut document = Document::from(buffer.into_vec());
    document.propagate_expand();

    Ok(Formatted::new(document, state.into_context()))
}

/// This structure stores the state that is relevant for the formatting of the whole document.
///
/// This structure is different from [`crate::Formatter`] in that the formatting infrastructure
/// creates a new [`crate::Formatter`] for every [`crate::write`!] call, whereas this structure stays alive
/// for the whole process of formatting a root with [`crate::format`!].
pub struct FormatState<Context> {
    context: Context,

    group_id_builder: UniqueGroupIdBuilder,
}

#[allow(clippy::missing_fields_in_debug)]
impl<Context> std::fmt::Debug for FormatState<Context>
where
    Context: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("FormatState")
            .field("context", &self.context)
            .finish()
    }
}

impl<Context> FormatState<Context> {
    /// Creates a new state with the given language specific context
    pub fn new(context: Context) -> Self {
        Self {
            context,
            group_id_builder: UniqueGroupIdBuilder::default(),
        }
    }

    pub fn into_context(self) -> Context {
        self.context
    }

    /// Returns the context specifying how to format the current CST
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// Returns a mutable reference to the context
    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    /// Creates a new group id that is unique to this document. The passed debug name is used in the
    /// [`std::fmt::Debug`] of the document if this is a debug build.
    /// The name is unused for production builds and has no meaning on the equality of two group ids.
    pub fn group_id(&self, debug_name: &'static str) -> GroupId {
        self.group_id_builder.group_id(debug_name)
    }
}
