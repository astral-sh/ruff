//! Infrastructure for code formatting
//!
//! This module defines [FormatElement], an IR to format code documents and provides a mean to print
//! such a document to a string. Objects that know how to format themselves implement the [Format] trait.
//!
//! ## Formatting Traits
//!
//! * [Format]: Implemented by objects that can be formatted.
//! * [FormatRule]: Rule that knows how to format an object of another type. Necessary in the situation where
//!  it's necessary to implement [Format] on an object from another crate. This module defines the
//!  [FormatRefWithRule] and [FormatOwnedWithRule] structs to pass an item with its corresponding rule.
//! * [FormatWithRule] implemented by objects that know how to format another type. Useful for implementing
//!  some reusable formatting logic inside of this module if the type itself doesn't implement [Format]
//!
//! ## Formatting Macros
//!
//! This crate defines two macros to construct the IR. These are inspired by Rust's `fmt` macros
//! * [`format!`]: Formats a formatable object
//! * [`format_args!`]: Concatenates a sequence of Format objects.
//! * [`write!`]: Writes a sequence of formatable objects into an output buffer.

#![allow(clippy::pedantic)]
#![deny(rustdoc::broken_intra_doc_links)]

mod arguments;
mod buffer;
mod builders;
pub mod comments;
pub mod diagnostics;
pub mod format_element;
mod format_extensions;
pub mod formatter;
pub mod group_id;
pub mod macros;
pub mod prelude;
#[cfg(debug_assertions)]
pub mod printed_tokens;
pub mod printer;
pub mod separated;
mod source_map;
pub mod token;
pub mod trivia;
mod verbatim;

use crate::formatter::Formatter;
use crate::group_id::UniqueGroupIdBuilder;
use crate::prelude::TagKind;
use std::fmt::Debug;

use crate::format_element::document::Document;
#[cfg(debug_assertions)]
use crate::printed_tokens::PrintedTokens;
use crate::printer::{Printer, PrinterOptions};
pub use arguments::{Argument, Arguments};
pub use buffer::{
    Buffer, BufferExtensions, BufferSnapshot, Inspect, PreambleBuffer, RemoveSoftLinesBuffer,
    VecBuffer,
};
pub use builders::BestFitting;

use crate::builders::syntax_token_cow_slice;
use crate::comments::{CommentStyle, Comments, SourceComment};
pub use crate::diagnostics::{ActualStart, FormatError, InvalidDocumentError, PrintError};
use crate::trivia::{format_skipped_token_trivia, format_trimmed_token};
pub use format_element::{normalize_newlines, FormatElement, LINE_TERMINATORS};
pub use group_id::GroupId;
use ruff_rowan::{
    Language, SyntaxElement, SyntaxNode, SyntaxResult, SyntaxToken, SyntaxTriviaPiece, TextLen,
    TextRange, TextSize, TokenAtOffset,
};
pub use source_map::{TransformSourceMap, TransformSourceMapBuilder};
use std::marker::PhantomData;
use std::num::ParseIntError;
use std::str::FromStr;

#[derive(Debug, Eq, PartialEq, Clone, Copy, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
#[derive(Default)]
pub enum IndentStyle {
    /// Tab
    #[default]
    Tab,
    /// Space, with its quantity
    Space(u8),
}

impl IndentStyle {
    pub const DEFAULT_SPACES: u8 = 2;

    /// Returns `true` if this is an [IndentStyle::Tab].
    pub const fn is_tab(&self) -> bool {
        matches!(self, IndentStyle::Tab)
    }

    /// Returns `true` if this is an [IndentStyle::Space].
    pub const fn is_space(&self) -> bool {
        matches!(self, IndentStyle::Space(_))
    }
}

impl FromStr for IndentStyle {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tab" | "Tabs" => Ok(Self::Tab),
            "space" | "Spaces" => Ok(Self::Space(IndentStyle::DEFAULT_SPACES)),
            // TODO: replace this error with a diagnostic
            _ => Err("Value not supported for IndentStyle"),
        }
    }
}

impl std::fmt::Display for IndentStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndentStyle::Tab => std::write!(f, "Tab"),
            IndentStyle::Space(size) => std::write!(f, "Spaces, size: {}", size),
        }
    }
}

/// Validated value for the `line_width` formatter options
///
/// The allowed range of values is 1..=320
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
pub struct LineWidth(u16);

impl LineWidth {
    /// Maximum allowed value for a valid [LineWidth]
    pub const MAX: u16 = 320;

    /// Return the numeric value for this [LineWidth]
    pub fn value(&self) -> u16 {
        self.0
    }
}

impl Default for LineWidth {
    fn default() -> Self {
        Self(80)
    }
}

/// Error type returned when parsing a [LineWidth] from a string fails
pub enum ParseLineWidthError {
    /// The string could not be parsed as a valid [u16]
    ParseError(ParseIntError),
    /// The [u16] value of the string is not a valid [LineWidth]
    TryFromIntError(LineWidthFromIntError),
}

impl Debug for ParseLineWidthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for ParseLineWidthError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseLineWidthError::ParseError(err) => std::fmt::Display::fmt(err, fmt),
            ParseLineWidthError::TryFromIntError(err) => std::fmt::Display::fmt(err, fmt),
        }
    }
}

impl FromStr for LineWidth {
    type Err = ParseLineWidthError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = u16::from_str(s).map_err(ParseLineWidthError::ParseError)?;
        let value = Self::try_from(value).map_err(ParseLineWidthError::TryFromIntError)?;
        Ok(value)
    }
}

/// Error type returned when converting a u16 to a [LineWidth] fails
#[derive(Clone, Copy, Debug)]
pub struct LineWidthFromIntError(pub u16);

impl TryFrom<u16> for LineWidth {
    type Error = LineWidthFromIntError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 0 && value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(LineWidthFromIntError(value))
        }
    }
}

impl std::fmt::Display for LineWidthFromIntError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "The line width exceeds the maximum value ({})",
            LineWidth::MAX
        )
    }
}

impl From<LineWidth> for u16 {
    fn from(value: LineWidth) -> Self {
        value.0
    }
}

/// Context object storing data relevant when formatting an object.
pub trait FormatContext {
    type Options: FormatOptions;

    /// Returns the formatting options
    fn options(&self) -> &Self::Options;

    /// Returns [None] if the CST has not been pre-processed.
    ///
    /// Returns [Some] if the CST has been pre-processed to simplify formatting.
    /// The source map can be used to map positions of the formatted nodes back to their original
    /// source locations or to resolve the source text.
    fn source_map(&self) -> Option<&TransformSourceMap>;
}

/// Options customizing how the source code should be formatted.
pub trait FormatOptions {
    /// The indent style.
    fn indent_style(&self) -> IndentStyle;

    /// What's the max width of a line. Defaults to 80.
    fn line_width(&self) -> LineWidth;

    /// Derives the print options from the these format options
    fn as_print_options(&self) -> PrinterOptions;
}

/// The [CstFormatContext] is an extension of the CST unaware [FormatContext] and must be implemented
/// by every language.
///
/// The context customizes the comments formatting and stores the comments of the CST.
pub trait CstFormatContext: FormatContext {
    type Language: Language;
    type Style: CommentStyle<Language = Self::Language>;

    /// Rule for formatting comments.
    type CommentRule: FormatRule<SourceComment<Self::Language>, Context = Self> + Default;

    /// Returns a reference to the program's comments.
    fn comments(&self) -> &Comments<Self::Language>;
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct SimpleFormatContext {
    options: SimpleFormatOptions,
}

impl SimpleFormatContext {
    pub fn new(options: SimpleFormatOptions) -> Self {
        Self { options }
    }
}

impl FormatContext for SimpleFormatContext {
    type Options = SimpleFormatOptions;

    fn options(&self) -> &Self::Options {
        &self.options
    }

    fn source_map(&self) -> Option<&TransformSourceMap> {
        None
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct SimpleFormatOptions {
    pub indent_style: IndentStyle,
    pub line_width: LineWidth,
}

impl FormatOptions for SimpleFormatOptions {
    fn indent_style(&self) -> IndentStyle {
        self.indent_style
    }

    fn line_width(&self) -> LineWidth {
        self.line_width
    }

    fn as_print_options(&self) -> PrinterOptions {
        PrinterOptions::default()
            .with_indent(self.indent_style)
            .with_print_width(self.line_width.into())
    }
}

/// Lightweight sourcemap marker between source and output tokens
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
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
        let print_options = self.context.options().as_print_options();

        let printed = Printer::new(print_options).print(&self.document)?;

        let printed = match self.context.source_map() {
            Some(source_map) => source_map.map_printed(printed),
            None => printed,
        };

        Ok(printed)
    }

    pub fn print_with_indent(&self, indent: u16) -> PrintResult<Printed> {
        let print_options = self.context.options().as_print_options();
        let printed = Printer::new(print_options).print_with_indent(&self.document, indent)?;

        let printed = match self.context.source_map() {
            Some(source_map) => source_map.map_printed(printed),
            None => printed,
        };

        Ok(printed)
    }
}
pub type PrintResult<T> = Result<T, PrintError>;

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)
)]
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

    /// Returns a list of [SourceMarker] mapping byte positions
    /// in the output string to the input source code.
    /// It's not guaranteed that the markers are sorted by source position.
    pub fn sourcemap(&self) -> &[SourceMarker] {
        &self.sourcemap
    }

    /// Returns a list of [SourceMarker] mapping byte positions
    /// in the output string to the input source code, consuming the result
    pub fn into_sourcemap(self) -> Vec<SourceMarker> {
        self.sourcemap
    }

    /// Takes the list of [SourceMarker] mapping byte positions in the output string
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
}

/// Public return type of the formatter
pub type FormatResult<F> = Result<F, FormatError>;

/// Formatting trait for types that can create a formatted representation. The `ruff_formatter` equivalent
/// to [std::fmt::Display].
///
/// ## Example
/// Implementing `Format` for a custom struct
///
/// ```
/// use ruff_formatter::{format, write, IndentStyle, LineWidth};
/// use ruff_formatter::prelude::*;
/// use ruff_rowan::TextSize;
///
/// struct Paragraph(String);
///
/// impl Format<SimpleFormatContext> for Paragraph {
///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
///         write!(f, [
///             hard_line_break(),
///             dynamic_text(&self.0, TextSize::from(0)),
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
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        Format::fmt(&**self, f)
    }
}

impl<T, Context> Format<Context> for &mut T
where
    T: ?Sized + Format<Context>,
{
    #[inline(always)]
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

impl<T, Context> Format<Context> for SyntaxResult<T>
where
    T: Format<Context>,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        match self {
            Ok(value) => value.fmt(f),
            Err(err) => Err(err.into()),
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
/// Implementing [Format] on the object itself is preferred over implementing [FormatRule] but
/// this isn't possible inside of a dependent crate for external type.
///
/// For example, the `ruff_js_formatter` crate isn't able to implement [Format] on `JsIfStatement`
/// because both the [Format] trait and `JsIfStatement` are external types (Rust's orphan rule).
///
/// That's why the `ruff_js_formatter` crate must define a new-type that implements the formatting
/// of `JsIfStatement`.
pub trait FormatRule<T> {
    type Context;

    fn fmt(&self, item: &T, f: &mut Formatter<Self::Context>) -> FormatResult<()>;
}

/// Default implementation for formatting a token
pub struct FormatToken<C> {
    context: PhantomData<C>,
}

impl<C> Default for FormatToken<C> {
    fn default() -> Self {
        Self {
            context: PhantomData,
        }
    }
}

impl<C> FormatRule<SyntaxToken<C::Language>> for FormatToken<C>
where
    C: CstFormatContext,
    C::Language: 'static,
{
    type Context = C;

    fn fmt(
        &self,
        token: &SyntaxToken<C::Language>,
        f: &mut Formatter<Self::Context>,
    ) -> FormatResult<()> {
        f.state_mut().track_token(token);

        crate::write!(
            f,
            [
                format_skipped_token_trivia(token),
                format_trimmed_token(token),
            ]
        )
    }
}

/// Rule that supports customizing how it formats an object of type `T`.
pub trait FormatRuleWithOptions<T>: FormatRule<T> {
    type Options;

    /// Returns a new rule that uses the given options to format an object.
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
/// This can be useful if you want to format a `SyntaxNode` inside ruff_formatter.. `SyntaxNode` doesn't implement [Format]
/// itself but the language specific crate implements `AsFormat` and `IntoFormat` for it and the returned [Format]
/// implement [FormatWithRule].
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
pub struct FormatRefWithRule<'a, T, R>
where
    R: FormatRule<T>,
{
    item: &'a T,
    rule: R,
}

impl<'a, T, R> FormatRefWithRule<'a, T, R>
where
    R: FormatRule<T>,
{
    pub fn new(item: &'a T, rule: R) -> Self {
        Self { item, rule }
    }
}

impl<T, R, O> FormatRefWithRule<'_, T, R>
where
    R: FormatRuleWithOptions<T, Options = O>,
{
    pub fn with_options(mut self, options: O) -> Self {
        self.rule = self.rule.with_options(options);
        self
    }
}

impl<T, R> FormatWithRule<R::Context> for FormatRefWithRule<'_, T, R>
where
    R: FormatRule<T>,
{
    type Item = T;

    fn item(&self) -> &Self::Item {
        self.item
    }
}

impl<T, R> Format<R::Context> for FormatRefWithRule<'_, T, R>
where
    R: FormatRule<T>,
{
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<R::Context>) -> FormatResult<()> {
        self.rule.fmt(self.item, f)
    }
}

/// Formats the `item` with the specified rule.
#[derive(Debug, Clone)]
pub struct FormatOwnedWithRule<T, R>
where
    R: FormatRule<T>,
{
    item: T,
    rule: R,
}

impl<T, R> FormatOwnedWithRule<T, R>
where
    R: FormatRule<T>,
{
    pub fn new(item: T, rule: R) -> Self {
        Self { item, rule }
    }

    pub fn with_item(mut self, item: T) -> Self {
        self.item = item;
        self
    }

    pub fn into_item(self) -> T {
        self.item
    }
}

impl<T, R> Format<R::Context> for FormatOwnedWithRule<T, R>
where
    R: FormatRule<T>,
{
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<R::Context>) -> FormatResult<()> {
        self.rule.fmt(&self.item, f)
    }
}

impl<T, R, O> FormatOwnedWithRule<T, R>
where
    R: FormatRuleWithOptions<T, Options = O>,
{
    pub fn with_options(mut self, options: O) -> Self {
        self.rule = self.rule.with_options(options);
        self
    }
}

impl<T, R> FormatWithRule<R::Context> for FormatOwnedWithRule<T, R>
where
    R: FormatRule<T>,
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
/// write!(&mut buffer, [format_args!(text("Hello World"))])?;
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
/// write!(&mut buffer, [text("Hello World")])?;
///
/// let formatted = Formatted::new(Document::from(buffer.into_vec()), SimpleFormatContext::default());
///
/// assert_eq!("Hello World", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
#[inline(always)]
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
/// let formatted = format!(SimpleFormatContext::default(), [&format_args!(text("test"))])?;
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
/// let formatted = format!(SimpleFormatContext::default(), [text("test")])?;
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
    let mut state = FormatState::new(context);
    let mut buffer = VecBuffer::with_capacity(arguments.items().len(), &mut state);

    buffer.write_fmt(arguments)?;

    let mut document = Document::from(buffer.into_vec());
    document.propagate_expand();

    Ok(Formatted::new(document, state.into_context()))
}

/// Entry point for formatting a [SyntaxNode] for a specific language.
pub trait FormatLanguage {
    type SyntaxLanguage: Language;

    /// The type of the formatting context
    type Context: CstFormatContext<Language = Self::SyntaxLanguage>;

    /// The rule type that can format a [SyntaxNode] of this language
    type FormatRule: FormatRule<SyntaxNode<Self::SyntaxLanguage>, Context = Self::Context> + Default;

    /// Performs an optional pre-processing of the tree. This can be useful to remove nodes
    /// that otherwise complicate formatting.
    ///
    /// Return [None] if the tree shouldn't be processed. Return [Some] with the transformed
    /// tree and the source map otherwise.
    fn transform(
        &self,
        _root: &SyntaxNode<Self::SyntaxLanguage>,
    ) -> Option<(SyntaxNode<Self::SyntaxLanguage>, TransformSourceMap)> {
        None
    }

    /// This is used to select appropriate "root nodes" for the
    /// range formatting process: for instance in JavaScript the function returns
    /// true for statement and declaration nodes, to ensure the entire statement
    /// gets formatted instead of the smallest sub-expression that fits the range
    fn is_range_formatting_node(&self, _node: &SyntaxNode<Self::SyntaxLanguage>) -> bool {
        true
    }

    /// Returns the formatting options
    fn options(&self) -> &<Self::Context as FormatContext>::Options;

    /// Creates the [FormatContext] with the given `source map` and `comments`
    fn create_context(
        self,
        root: &SyntaxNode<Self::SyntaxLanguage>,
        source_map: Option<TransformSourceMap>,
    ) -> Self::Context;
}

/// Formats a syntax node file based on its features.
///
/// It returns a [Formatted] result, which the user can use to override a file.
pub fn format_node<L: FormatLanguage>(
    root: &SyntaxNode<L::SyntaxLanguage>,
    language: L,
) -> FormatResult<Formatted<L::Context>> {
    tracing::trace_span!("format_node").in_scope(move || {
        let (root, source_map) = match language.transform(root) {
            Some((root, source_map)) => (root, Some(source_map)),
            None => (root.clone(), None),
        };

        let context = language.create_context(&root, source_map);
        let format_node = FormatRefWithRule::new(&root, L::FormatRule::default());

        let mut state = FormatState::new(context);
        let mut buffer = VecBuffer::new(&mut state);

        write!(buffer, [format_node])?;

        let mut document = Document::from(buffer.into_vec());
        document.propagate_expand();

        state.assert_formatted_all_tokens(&root);

        let context = state.into_context();
        let comments = context.comments();

        comments.assert_checked_all_suppressions(&root);
        comments.assert_formatted_all_comments();

        Ok(Formatted::new(document, context))
    })
}

/// Returns the [TextRange] for this [SyntaxElement] with the leading and
/// trailing whitespace trimmed (but keeping comments or skipped trivias)
fn text_non_whitespace_range<E, L>(elem: &E) -> TextRange
where
    E: Into<SyntaxElement<L>> + Clone,
    L: Language,
{
    let elem: SyntaxElement<L> = elem.clone().into();

    let start = elem
        .leading_trivia()
        .into_iter()
        .flat_map(|trivia| trivia.pieces())
        .find_map(|piece| {
            if piece.is_whitespace() || piece.is_newline() {
                None
            } else {
                Some(piece.text_range().start())
            }
        })
        .unwrap_or_else(|| elem.text_trimmed_range().start());

    let end = elem
        .trailing_trivia()
        .into_iter()
        .flat_map(|trivia| trivia.pieces().rev())
        .find_map(|piece| {
            if piece.is_whitespace() || piece.is_newline() {
                None
            } else {
                Some(piece.text_range().end())
            }
        })
        .unwrap_or_else(|| elem.text_trimmed_range().end());

    TextRange::new(start, end)
}

/// Formats a range within a file, supported by Rome
///
/// This runs a simple heuristic to determine the initial indentation
/// level of the node based on the provided [FormatContext], which
/// must match currently the current initial of the file. Additionally,
/// because the reformatting happens only locally the resulting code
/// will be indented with the same level as the original selection,
/// even if it's a mismatch from the rest of the block the selection is in
///
/// It returns a [Formatted] result with a range corresponding to the
/// range of the input that was effectively overwritten by the formatter
pub fn format_range<Language: FormatLanguage>(
    root: &SyntaxNode<Language::SyntaxLanguage>,
    mut range: TextRange,
    language: Language,
) -> FormatResult<Printed> {
    if range.is_empty() {
        return Ok(Printed::new(
            String::new(),
            Some(range),
            Vec::new(),
            Vec::new(),
        ));
    }

    let root_range = root.text_range();
    if range.start() < root_range.start() || range.end() > root_range.end() {
        return Err(FormatError::RangeError {
            input: range,
            tree: root_range,
        });
    }

    // Find the tokens corresponding to the start and end of the range
    let start_token = root.token_at_offset(range.start());
    let end_token = root.token_at_offset(range.end());

    // If these tokens were not found this means either:
    // 1. The input [SyntaxNode] was empty
    // 2. The input node was not the root [SyntaxNode] of the file
    // In the first case we can return an empty result immediately,
    // otherwise default to the first and last tokens in the root node
    let mut start_token = match start_token {
        // If the start of the range lies between two tokens,
        // start at the rightmost one
        TokenAtOffset::Between(_, token) => token,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::None => match root.first_token() {
            Some(token) => token,
            // root node is empty
            None => return Ok(Printed::new_empty()),
        },
    };
    let mut end_token = match end_token {
        // If the end of the range lies between two tokens,
        // end at the leftmost one
        TokenAtOffset::Between(token, _) => token,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::None => match root.last_token() {
            Some(token) => token,
            // root node is empty
            None => return Ok(Printed::new_empty()),
        },
    };

    // Trim leading and trailing whitespace off from the formatting range
    let mut trimmed_start = range.start();

    let start_token_range = text_non_whitespace_range(&start_token);
    let start_token_trimmed_start = start_token_range.start();
    let start_token_trimmed_end = start_token_range.end();

    if start_token_trimmed_start >= range.start() && start_token_trimmed_start <= range.end() {
        // If the range starts before the trimmed start of the token, move the
        // start towards that position
        trimmed_start = start_token_trimmed_start;
    } else if start_token_trimmed_end <= range.start() {
        // If the range starts after the trimmed end of the token, move the
        // start to the trimmed start of the next token if it exists
        if let Some(next_token) = start_token.next_token() {
            let next_token_start = text_non_whitespace_range(&next_token).start();
            if next_token_start <= range.end() {
                trimmed_start = next_token_start;
                start_token = next_token;
            }
        }
    }

    let end_token_range = text_non_whitespace_range(&end_token);
    let end_token_trimmed_start = end_token_range.start();

    // If the range ends before the trimmed start of the token, move the
    // end to the trimmed end of the previous token if it exists
    if end_token_trimmed_start >= range.end() {
        if let Some(next_token) = end_token.prev_token() {
            let next_token_end = text_non_whitespace_range(&next_token).end();
            if next_token_end >= trimmed_start {
                end_token = next_token;
            }
        }
    }

    // Find suitable formatting-root nodes (matching the predicate provided by
    // the language implementation) in the ancestors of the start and end tokens
    let start_node = start_token
        .ancestors()
        .find(|node| language.is_range_formatting_node(node))
        .unwrap_or_else(|| root.clone());
    let end_node = end_token
        .ancestors()
        .find(|node| language.is_range_formatting_node(node))
        .unwrap_or_else(|| root.clone());

    let common_root = if start_node == end_node {
        range = text_non_whitespace_range(&start_node);
        Some(start_node)
    } else {
        // Find the two highest sibling nodes that satisfy the formatting range
        // from the ancestors of the start and end nodes (this is roughly the
        // same algorithm as the findSiblingAncestors function in Prettier, see
        // https://github.com/prettier/prettier/blob/cae195187f524dd74e60849e0a4392654423415b/src/main/range-util.js#L36)
        let start_node_start = start_node.text_range().start();
        let end_node_end = end_node.text_range().end();

        let result_end_node = end_node
            .ancestors()
            .take_while(|end_parent| end_parent.text_range().start() >= start_node_start)
            .last()
            .unwrap_or(end_node);

        let result_start_node = start_node
            .ancestors()
            .take_while(|start_parent| start_parent.text_range().end() <= end_node_end)
            .last()
            .unwrap_or(start_node);

        range = text_non_whitespace_range(&result_start_node)
            .cover(text_non_whitespace_range(&result_end_node));

        // Find the lowest common ancestor node for the previously selected
        // sibling nodes by building the path to the root node from both
        // nodes and iterating along the two paths at once to find the first
        // divergence (the ancestors have to be collected into vectors first
        // since the ancestor iterator isn't double ended)
        #[allow(clippy::needless_collect)]
        let start_to_root: Vec<_> = result_start_node.ancestors().collect();
        #[allow(clippy::needless_collect)]
        let end_to_root: Vec<_> = result_end_node.ancestors().collect();

        start_to_root
            .into_iter()
            .rev()
            .zip(end_to_root.into_iter().rev())
            .map_while(|(lhs, rhs)| if lhs == rhs { Some(lhs) } else { None })
            .last()
    };

    // Logically this should always return at least the root node,
    // fallback to said node just in case
    let common_root = common_root.as_ref().unwrap_or(root);

    // Perform the actual formatting of the root node with
    // an appropriate indentation level
    let mut printed = format_sub_tree(common_root, language)?;

    // This finds the closest marker to the beginning of the source
    // starting before or at said starting point, and the closest
    // marker to the end of the source range starting after or at
    // said ending point respectively
    let mut range_start = None;
    let mut range_end = None;

    let sourcemap = printed.sourcemap();
    for marker in sourcemap {
        // marker.source <= range.start()
        if let Some(start_dist) = range.start().checked_sub(marker.source) {
            range_start = match range_start {
                Some((prev_marker, prev_dist)) => {
                    if start_dist < prev_dist {
                        Some((marker, start_dist))
                    } else {
                        Some((prev_marker, prev_dist))
                    }
                }
                None => Some((marker, start_dist)),
            }
        }

        // marker.source >= range.end()
        if let Some(end_dist) = marker.source.checked_sub(range.end()) {
            range_end = match range_end {
                Some((prev_marker, prev_dist)) => {
                    if end_dist <= prev_dist {
                        Some((marker, end_dist))
                    } else {
                        Some((prev_marker, prev_dist))
                    }
                }
                None => Some((marker, end_dist)),
            }
        }
    }

    // If no start or end were found, this means that the edge of the formatting
    // range was near the edge of the input, and no marker were emitted before
    // the start (or after the end) of the formatting range: in this case
    // the start/end marker default to the start/end of the input
    let (start_source, start_dest) = match range_start {
        Some((start_marker, _)) => (start_marker.source, start_marker.dest),
        None => (common_root.text_range().start(), TextSize::from(0)),
    };
    let (end_source, end_dest) = match range_end {
        Some((end_marker, _)) => (end_marker.source, end_marker.dest),
        None => (
            common_root.text_range().end(),
            TextSize::try_from(printed.as_code().len()).expect("code length out of bounds"),
        ),
    };

    let input_range = TextRange::new(start_source, end_source);
    let output_range = TextRange::new(start_dest, end_dest);
    let sourcemap = printed.take_sourcemap();
    let verbatim_ranges = printed.take_verbatim_ranges();
    let code = &printed.into_code()[output_range];
    Ok(Printed::new(
        code.into(),
        Some(input_range),
        sourcemap,
        verbatim_ranges,
    ))
}

/// Formats a single node within a file, supported by Rome.
///
/// This runs a simple heuristic to determine the initial indentation
/// level of the node based on the provided [FormatContext], which
/// must match currently the current initial of the file. Additionally,
/// because the reformatting happens only locally the resulting code
/// will be indented with the same level as the original selection,
/// even if it's a mismatch from the rest of the block the selection is in
///
/// It returns a [Formatted] result
pub fn format_sub_tree<L: FormatLanguage>(
    root: &SyntaxNode<L::SyntaxLanguage>,
    language: L,
) -> FormatResult<Printed> {
    // Determine the initial indentation level for the printer by inspecting the trivia pieces
    // of each token from the first token of the common root towards the start of the file
    let mut tokens = std::iter::successors(root.first_token(), |token| token.prev_token());

    // From the iterator of tokens, build an iterator of trivia pieces (once again the iterator is
    // reversed, starting from the last trailing trivia towards the first leading trivia).
    // The first token is handled specially as we only wan to consider its leading trivia pieces
    let first_token = tokens.next();
    let first_token_trivias = first_token
        .into_iter()
        .flat_map(|token| token.leading_trivia().pieces().rev());

    let next_tokens_trivias = tokens.flat_map(|token| {
        token
            .trailing_trivia()
            .pieces()
            .rev()
            .chain(token.leading_trivia().pieces().rev())
    });

    let trivias = first_token_trivias
        .chain(next_tokens_trivias)
        .filter(|piece| {
            // We're only interested in newline and whitespace trivias, skip over comments
            let is_newline = piece.is_newline();
            let is_whitespace = piece.is_whitespace();
            is_newline || is_whitespace
        });

    // Finally run the iterator until a newline trivia is found, and get the last whitespace trivia before it
    let last_whitespace = trivias.map_while(|piece| piece.as_whitespace()).last();
    let initial_indent = match last_whitespace {
        Some(trivia) => {
            // This logic is based on the formatting options passed in
            // the be user (or the editor) as we do not have any kind
            // of indentation type detection yet. Unfortunately this
            // may not actually match the current content of the file
            let length = trivia.text().len() as u16;
            match language.options().indent_style() {
                IndentStyle::Tab => length,
                IndentStyle::Space(width) => length / u16::from(width),
            }
        }
        // No whitespace was found between the start of the range
        // and the start of the file
        None => 0,
    };

    let formatted = format_node(root, language)?;
    let mut printed = formatted.print_with_indent(initial_indent)?;
    let sourcemap = printed.take_sourcemap();
    let verbatim_ranges = printed.take_verbatim_ranges();

    Ok(Printed::new(
        printed.into_code(),
        Some(root.text_range()),
        sourcemap,
        verbatim_ranges,
    ))
}

impl<L: Language, Context> Format<Context> for SyntaxTriviaPiece<L> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let range = self.text_range();

        // Trim start/end and update the range
        let trimmed = self.text().trim_start();
        let trimmed_start = range.start() + (range.len() - trimmed.text_len());
        let trimmed = trimmed.trim_end();

        write!(
            f,
            [syntax_token_cow_slice(
                normalize_newlines(trimmed, LINE_TERMINATORS),
                &self.token(),
                trimmed_start
            )]
        )
    }
}

/// This structure stores the state that is relevant for the formatting of the whole document.
///
/// This structure is different from [crate::Formatter] in that the formatting infrastructure
/// creates a new [crate::Formatter] for every [crate::write!] call, whereas this structure stays alive
/// for the whole process of formatting a root with [crate::format!].
pub struct FormatState<Context> {
    context: Context,

    group_id_builder: UniqueGroupIdBuilder,

    // This is using a RefCell as it only exists in debug mode,
    // the Formatter is still completely immutable in release builds
    #[cfg(debug_assertions)]
    pub printed_tokens: PrintedTokens,
}

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
            group_id_builder: Default::default(),

            #[cfg(debug_assertions)]
            printed_tokens: Default::default(),
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
    /// [std::fmt::Debug] of the document if this is a debug build.
    /// The name is unused for production builds and has no meaning on the equality of two group ids.
    pub fn group_id(&self, debug_name: &'static str) -> GroupId {
        self.group_id_builder.group_id(debug_name)
    }

    /// Tracks the given token as formatted
    #[inline]
    pub fn track_token<L: Language>(&mut self, #[allow(unused_variables)] token: &SyntaxToken<L>) {
        cfg_if::cfg_if! {
            if #[cfg(debug_assertions)] {
                self.printed_tokens.track_token(token);
            }
        }
    }

    #[cfg(not(debug_assertions))]
    #[inline]
    pub fn set_token_tracking_disabled(&mut self, _: bool) {}

    /// Disables or enables token tracking for a portion of the code.
    ///
    /// It can be useful to disable the token tracking when it is necessary to re-format a node with different parameters.
    #[cfg(debug_assertions)]
    pub fn set_token_tracking_disabled(&mut self, enabled: bool) {
        self.printed_tokens.set_disabled(enabled)
    }

    #[cfg(not(debug_assertions))]
    #[inline]
    pub fn is_token_tracking_disabled(&self) -> bool {
        false
    }

    /// Returns `true` if token tracking is currently disabled.
    #[cfg(debug_assertions)]
    pub fn is_token_tracking_disabled(&self) -> bool {
        self.printed_tokens.is_disabled()
    }

    /// Asserts in debug builds that all tokens have been printed.
    #[inline]
    pub fn assert_formatted_all_tokens<L: Language>(
        &self,
        #[allow(unused_variables)] root: &SyntaxNode<L>,
    ) {
        cfg_if::cfg_if! {
            if #[cfg(debug_assertions)] {
                self.printed_tokens.assert_all_tracked(root);
            }
        }
    }
}

impl<Context> FormatState<Context>
where
    Context: FormatContext,
{
    pub fn snapshot(&self) -> FormatStateSnapshot {
        FormatStateSnapshot {
            #[cfg(debug_assertions)]
            printed_tokens: self.printed_tokens.snapshot(),
        }
    }

    pub fn restore_snapshot(&mut self, snapshot: FormatStateSnapshot) {
        let FormatStateSnapshot {
            #[cfg(debug_assertions)]
            printed_tokens,
        } = snapshot;

        cfg_if::cfg_if! {
            if #[cfg(debug_assertions)] {
                self.printed_tokens.restore(printed_tokens);
            }
        }
    }
}

pub struct FormatStateSnapshot {
    #[cfg(debug_assertions)]
    printed_tokens: printed_tokens::PrintedTokensSnapshot,
}
