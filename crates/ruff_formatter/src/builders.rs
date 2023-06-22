use crate::format_element::tag::{Condition, Tag};
use crate::prelude::tag::{DedentMode, GroupMode, LabelId};
use crate::prelude::*;
use crate::{format_element, write, Argument, Arguments, FormatContext, GroupId, TextSize};
use crate::{Buffer, VecBuffer};

use ruff_text_size::TextRange;
use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroU8;
use Tag::*;

/// A line break that only gets printed if the enclosing `Group` doesn't fit on a single line.
/// It's omitted if the enclosing `Group` fits on a single line.
/// A soft line break is identical to a hard line break when not enclosed inside of a `Group`.
///
/// # Examples
///
/// Soft line breaks are omitted if the enclosing `Group` fits on a single line
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![text("a,"), soft_line_break(), text("b")])
/// ])?;
///
/// assert_eq!(
///     "a,b",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
/// See [soft_line_break_or_space] if you want to insert a space between the elements if the enclosing
/// `Group` fits on a single line.
///
/// Soft line breaks are emitted if the enclosing `Group` doesn't fit on a single line
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(10).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("a long word,"),
///         soft_line_break(),
///         text("so that the group doesn't fit on a single line"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "a long word,\nso that the group doesn't fit on a single line",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub const fn soft_line_break() -> Line {
    Line::new(LineMode::Soft)
}

/// A forced line break that are always printed. A hard line break forces any enclosing `Group`
/// to be printed over multiple lines.
///
/// # Examples
///
/// It forces a line break, even if the enclosing `Group` would otherwise fit on a single line.
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("a,"),
///         hard_line_break(),
///         text("b"),
///         hard_line_break()
///     ])
/// ])?;
///
/// assert_eq!(
///     "a,\nb\n",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub const fn hard_line_break() -> Line {
    Line::new(LineMode::Hard)
}

/// A forced empty line. An empty line inserts enough line breaks in the output for
/// the previous and next element to be separated by an empty line.
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// fn main() -> FormatResult<()> {
/// let elements = format!(
///     SimpleFormatContext::default(), [
///     group(&format_args![
///         text("a,"),
///         empty_line(),
///         text("b"),
///         empty_line()
///     ])
/// ])?;
///
/// assert_eq!(
///     "a,\n\nb\n\n",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub const fn empty_line() -> Line {
    Line::new(LineMode::Empty)
}

/// A line break if the enclosing `Group` doesn't fit on a single line, a space otherwise.
///
/// # Examples
///
/// The line breaks are emitted as spaces if the enclosing `Group` fits on a a single line:
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("a,"),
///         soft_line_break_or_space(),
///         text("b"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "a, b",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// The printer breaks the lines if the enclosing `Group` doesn't fit on a single line:
/// ```
/// use ruff_formatter::{format_args, format, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(10).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("a long word,"),
///         soft_line_break_or_space(),
///         text("so that the group doesn't fit on a single line"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "a long word,\nso that the group doesn't fit on a single line",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub const fn soft_line_break_or_space() -> Line {
    Line::new(LineMode::SoftOrSpace)
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Line {
    mode: LineMode,
}

impl Line {
    const fn new(mode: LineMode) -> Self {
        Self { mode }
    }
}

impl<Context> Format<Context> for Line {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Line(self.mode))
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Line").field(&self.mode).finish()
    }
}

/// Creates a token that gets written as is to the output. Make sure to properly escape the text if
/// it's user generated (e.g. a string and not a language keyword).
///
/// # Line feeds
/// Tokens may contain line breaks but they must use the line feeds (`\n`).
/// The [crate::Printer] converts the line feed characters to the character specified in the [crate::PrinterOptions].
///
/// # Examples
///
/// ```
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [text("Hello World")])?;
///
/// assert_eq!(
///     "Hello World",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// Printing a string literal as a literal requires that the string literal is properly escaped and
/// enclosed in quotes (depending on the target language).
///
/// ```
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// // the tab must be encoded as \\t to not literally print a tab character ("Hello{tab}World" vs "Hello\tWorld")
/// let elements = format!(SimpleFormatContext::default(), [text("\"Hello\\tWorld\"")])?;
///
/// assert_eq!(r#""Hello\tWorld""#, elements.print()?.as_code());
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn text(text: &'static str) -> StaticText {
    debug_assert_no_newlines(text);

    StaticText { text }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct StaticText {
    text: &'static str,
}

impl<Context> Format<Context> for StaticText {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::StaticText { text: self.text })
    }
}

impl std::fmt::Debug for StaticText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::write!(f, "StaticToken({})", self.text)
    }
}

/// Creates a source map entry from the passed source `position` to the position in the formatted output.
///
/// ## Examples
///
/// ```
/// /// ```
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// // the tab must be encoded as \\t to not literally print a tab character ("Hello{tab}World" vs "Hello\tWorld")
/// use ruff_text_size::TextSize;
/// use ruff_formatter::SourceMarker;
///
///
/// let elements = format!(SimpleFormatContext::default(), [
///     source_position(TextSize::new(0)),
///     text("\"Hello "),
///     source_position(TextSize::new(8)),
///     text("'Ruff'"),
///     source_position(TextSize::new(14)),
///     text("\""),
///     source_position(TextSize::new(20))
/// ])?;
///
/// let printed = elements.print()?;
///
/// assert_eq!(printed.as_code(), r#""Hello 'Ruff'""#);
/// assert_eq!(printed.sourcemap(), [
///     SourceMarker { source: TextSize::new(0), dest: TextSize::new(0) },
///     SourceMarker { source: TextSize::new(0), dest: TextSize::new(7) },
///     SourceMarker { source: TextSize::new(8), dest: TextSize::new(7) },
///     SourceMarker { source: TextSize::new(8), dest: TextSize::new(13) },
///     SourceMarker { source: TextSize::new(14), dest: TextSize::new(13) },
///     SourceMarker { source: TextSize::new(14), dest: TextSize::new(14) },
///     SourceMarker { source: TextSize::new(20), dest: TextSize::new(14) },
/// ]);
///
/// # Ok(())
/// # }
/// ```
pub const fn source_position(position: TextSize) -> SourcePosition {
    SourcePosition(position)
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct SourcePosition(TextSize);

impl<Context> Format<Context> for SourcePosition {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::SourcePosition(self.0))
    }
}

/// Creates a text from a dynamic string with its optional start-position in the source document
pub fn dynamic_text(text: &str, position: Option<TextSize>) -> DynamicText {
    debug_assert_no_newlines(text);

    DynamicText { text, position }
}

#[derive(Eq, PartialEq)]
pub struct DynamicText<'a> {
    text: &'a str,
    position: Option<TextSize>,
}

impl<Context> Format<Context> for DynamicText<'_> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        if let Some(source_position) = self.position {
            f.write_element(FormatElement::SourcePosition(source_position))?;
        }

        f.write_element(FormatElement::DynamicText {
            text: self.text.to_string().into_boxed_str(),
        })
    }
}

impl std::fmt::Debug for DynamicText<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::write!(f, "DynamicToken({})", self.text)
    }
}

/// Emits a text as it is written in the source document. Optimized to avoid allocations.
pub const fn source_text_slice(
    range: TextRange,
    newlines: ContainsNewlines,
) -> SourceTextSliceBuilder {
    SourceTextSliceBuilder {
        range,
        new_lines: newlines,
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ContainsNewlines {
    /// The string contains newline characters
    Yes,
    /// The string contains no newline characters
    No,

    /// The string may contain newline characters, search the string to determine if there are any newlines.
    Detect,
}

#[derive(Eq, PartialEq, Debug)]
pub struct SourceTextSliceBuilder {
    range: TextRange,
    new_lines: ContainsNewlines,
}

impl<Context> Format<Context> for SourceTextSliceBuilder
where
    Context: FormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let source_code = f.context().source_code();
        let slice = source_code.slice(self.range);
        debug_assert_no_newlines(slice.text(source_code));

        let contains_newlines = match self.new_lines {
            ContainsNewlines::Yes => {
                debug_assert!(
                    slice.text(source_code).contains('\n'),
                    "Text contains no new line characters but the caller specified that it does."
                );
                true
            }
            ContainsNewlines::No => {
                debug_assert!(
                    !slice.text(source_code).contains('\n'),
                    "Text contains new line characters but the caller specified that it does not."
                );
                false
            }
            ContainsNewlines::Detect => slice.text(source_code).contains('\n'),
        };

        f.write_element(FormatElement::SourceCodeSlice {
            slice,
            contains_newlines,
        })
    }
}

fn debug_assert_no_newlines(text: &str) {
    debug_assert!(!text.contains('\r'), "The content '{text}' contains an unsupported '\\r' line terminator character but text must only use line feeds '\\n' as line separator. Use '\\n' instead of '\\r' and '\\r\\n' to insert a line break in strings.");
}

/// Pushes some content to the end of the current line
///
/// ## Examples
///
/// ```
/// use ruff_formatter::{format};
/// use ruff_formatter::prelude::*;
///
/// fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     text("a"),
///     line_suffix(&text("c")),
///     text("b")
/// ])?;
///
/// assert_eq!(
///     "abc",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn line_suffix<Content, Context>(inner: &Content) -> LineSuffix<Context>
where
    Content: Format<Context>,
{
    LineSuffix {
        content: Argument::new(inner),
    }
}

#[derive(Copy, Clone)]
pub struct LineSuffix<'a, Context> {
    content: Argument<'a, Context>,
}

impl<Context> Format<Context> for LineSuffix<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartLineSuffix))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndLineSuffix))
    }
}

impl<Context> std::fmt::Debug for LineSuffix<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LineSuffix").field(&"{{content}}").finish()
    }
}

/// Inserts a boundary for line suffixes that forces the printer to print all pending line suffixes.
/// Helpful if a line sufix shouldn't pass a certain point.
///
/// ## Examples
///
/// Forces the line suffix "c" to be printed before the token `d`.
/// ```
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn  main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     text("a"),
///     line_suffix(&text("c")),
///     text("b"),
///     line_suffix_boundary(),
///     text("d")
/// ])?;
///
/// assert_eq!(
///     "abc\nd",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
pub const fn line_suffix_boundary() -> LineSuffixBoundary {
    LineSuffixBoundary
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct LineSuffixBoundary;

impl<Context> Format<Context> for LineSuffixBoundary {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::LineSuffixBoundary)
    }
}

/// Marks some content with a label.
///
/// This does not directly influence how this content will be printed, but some
/// parts of the formatter may inspect the [labelled element](Tag::StartLabelled)
/// using [FormatElements::has_label].
///
/// ## Examples
///
/// ```rust
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{format, write, LineWidth};
///
/// #[derive(Debug, Copy, Clone)]
/// enum MyLabels {
///     Main
/// }
///
/// impl tag::LabelDefinition for MyLabels {
///     fn value(&self) -> u64 {
///         *self as u64
///     }
///
///     fn name(&self) -> &'static str {
///         match self {
///             Self::Main => "Main"
///         }
///     }
/// }
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(
///     SimpleFormatContext::default(),
///     [format_with(|f| {
///         let mut recording = f.start_recording();
///         write!(recording, [
///             labelled(
///                 LabelId::of(MyLabels::Main),
///                 &text("'I have a label'")
///             )
///         ])?;
///
///         let recorded = recording.stop();
///
///         let is_labelled = recorded.first().map_or(false, |element| element.has_label(LabelId::of(MyLabels::Main)));
///
///         if is_labelled {
///             write!(f, [text(" has label `Main`")])
///         } else {
///             write!(f, [text(" doesn't have label `Main`")])
///         }
///     })]
/// )?;
///
/// assert_eq!("'I have a label' has label `Main`", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
/// ## Alternatives
///
/// Use `Memoized.inspect(f)?.has_label(LabelId::of::<SomeLabelId>()` if you need to know if some content breaks that should
/// only be written later.
#[inline]
pub fn labelled<Content, Context>(label_id: LabelId, content: &Content) -> FormatLabelled<Context>
where
    Content: Format<Context>,
{
    FormatLabelled {
        label_id,
        content: Argument::new(content),
    }
}

#[derive(Copy, Clone)]
pub struct FormatLabelled<'a, Context> {
    label_id: LabelId,
    content: Argument<'a, Context>,
}

impl<Context> Format<Context> for FormatLabelled<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartLabelled(self.label_id)))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndLabelled))
    }
}

impl<Context> std::fmt::Debug for FormatLabelled<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Label")
            .field(&self.label_id)
            .field(&"{{content}}")
            .finish()
    }
}

/// Inserts a single space. Allows to separate different tokens.
///
/// # Examples
///
/// ```
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// // the tab must be encoded as \\t to not literally print a tab character ("Hello{tab}World" vs "Hello\tWorld")
/// let elements = format!(SimpleFormatContext::default(), [text("a"), space(), text("b")])?;
///
/// assert_eq!("a b", elements.print()?.as_code());
/// # Ok(())
/// # }
/// ```
#[inline]
pub const fn space() -> Space {
    Space
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Space;

impl<Context> Format<Context> for Space {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Space)
    }
}

/// It adds a level of indentation to the given content
///
/// It doesn't add any line breaks at the edges of the content, meaning that
/// the line breaks have to be manually added.
///
/// This helper should be used only in rare cases, instead you should rely more on
/// [block_indent] and [soft_block_indent]
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     text("switch {"),
///     block_indent(&format_args![
///         text("default:"),
///         indent(&format_args![
///             // this is where we want to use a
///             hard_line_break(),
///             text("break;"),
///         ])
///     ]),
///     text("}"),
/// ])?;
///
/// assert_eq!(
///     "switch {\n\tdefault:\n\t\tbreak;\n}",
///     block.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn indent<Content, Context>(content: &Content) -> Indent<Context>
where
    Content: Format<Context>,
{
    Indent {
        content: Argument::new(content),
    }
}

#[derive(Copy, Clone)]
pub struct Indent<'a, Context> {
    content: Argument<'a, Context>,
}

impl<Context> Format<Context> for Indent<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartIndent))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndIndent))
    }
}

impl<Context> std::fmt::Debug for Indent<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Indent").field(&"{{content}}").finish()
    }
}

/// It reduces the indention for the given content depending on the closest [indent] or [align] parent element.
/// - [align] Undoes the spaces added by [align]
/// - [indent] Reduces the indention level by one
///
/// This is a No-op if the indention level is zero.
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     text("root"),
///     align(2, &format_args![
///         hard_line_break(),
///         text("aligned"),
///         dedent(&format_args![
///             hard_line_break(),
///             text("not aligned"),
///         ]),
///         dedent(&indent(&format_args![
///             hard_line_break(),
///             text("Indented, not aligned")
///         ]))
///     ]),
///     dedent(&format_args![
///         hard_line_break(),
///         text("Dedent on root level is a no-op.")
///     ])
/// ])?;
///
/// assert_eq!(
///     "root\n  aligned\nnot aligned\n\tIndented, not aligned\nDedent on root level is a no-op.",
///     block.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn dedent<Content, Context>(content: &Content) -> Dedent<Context>
where
    Content: Format<Context>,
{
    Dedent {
        content: Argument::new(content),
        mode: DedentMode::Level,
    }
}

#[derive(Copy, Clone)]
pub struct Dedent<'a, Context> {
    content: Argument<'a, Context>,
    mode: DedentMode,
}

impl<Context> Format<Context> for Dedent<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartDedent(self.mode)))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndDedent))
    }
}

impl<Context> std::fmt::Debug for Dedent<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Dedent").field(&"{{content}}").finish()
    }
}

/// It resets the indent document so that the content will be printed at the start of the line.
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     text("root"),
///     indent(&format_args![
///         hard_line_break(),
///         text("indent level 1"),
///         indent(&format_args![
///             hard_line_break(),
///             text("indent level 2"),
///             align(2, &format_args![
///                 hard_line_break(),
///                 text("two space align"),
///                 dedent_to_root(&format_args![
///                     hard_line_break(),
///                     text("starts at the beginning of the line")
///                 ]),
///             ]),
///             hard_line_break(),
///             text("end indent level 2"),
///         ])
///  ]),
/// ])?;
///
/// assert_eq!(
///     "root\n\tindent level 1\n\t\tindent level 2\n\t\t  two space align\nstarts at the beginning of the line\n\t\tend indent level 2",
///     block.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// ## Prettier
///
/// This resembles the behaviour of Prettier's `align(Number.NEGATIVE_INFINITY, content)` IR element.
#[inline]
pub fn dedent_to_root<Content, Context>(content: &Content) -> Dedent<Context>
where
    Content: Format<Context>,
{
    Dedent {
        content: Argument::new(content),
        mode: DedentMode::Root,
    }
}

/// Aligns its content by indenting the content by `count` spaces.
///
/// [align] is a variant of `[indent]` that indents its content by a specified number of spaces rather than
/// using the configured indent character (tab or a specified number of spaces).
///
/// You should use [align] when you want to indent a content by a specific number of spaces.
/// Using [indent] is preferred in all other situations as it respects the users preferred indent character.
///
/// # Examples
///
/// ## Tab indention
///
/// ```
/// use std::num::NonZeroU8;
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     text("a"),
///     hard_line_break(),
///     text("?"),
///     space(),
///     align(2, &format_args![
///         text("function () {"),
///         hard_line_break(),
///         text("}"),
///     ]),
///     hard_line_break(),
///     text(":"),
///     space(),
///     align(2, &format_args![
///         text("function () {"),
///         block_indent(&text("console.log('test');")),
///         text("}"),
///     ]),
///     text(";")
/// ])?;
///
/// assert_eq!(
///     "a\n? function () {\n  }\n: function () {\n\t\tconsole.log('test');\n  };",
///     block.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// You can see that:
///
/// - the printer indents the function's `}` by two spaces because it is inside of an `align`.
/// - the block `console.log` gets indented by two tabs.
///   This is because `align` increases the indention level by one (same as `indent`)
///   if you nest an `indent` inside an `align`.
///   Meaning that, `align > ... > indent` results in the same indention as `indent > ... > indent`.
///
/// ## Spaces indention
///
/// ```
/// use std::num::NonZeroU8;
/// use ruff_formatter::{format, format_args, IndentStyle, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     indent_style: IndentStyle::Space(4),
///     ..SimpleFormatOptions::default()
/// });
///
/// let block = format!(context, [
///     text("a"),
///     hard_line_break(),
///     text("?"),
///     space(),
///     align(2, &format_args![
///         text("function () {"),
///         hard_line_break(),
///         text("}"),
///     ]),
///     hard_line_break(),
///     text(":"),
///     space(),
///     align(2, &format_args![
///         text("function () {"),
///         block_indent(&text("console.log('test');")),
///         text("}"),
///     ]),
///     text(";")
/// ])?;
///
/// assert_eq!(
///     "a\n? function () {\n  }\n: function () {\n      console.log('test');\n  };",
///     block.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// The printing of `align` differs if using spaces as indention sequence *and* it contains an `indent`.
/// You can see the difference when comparing the indention of the `console.log(...)` expression to the previous example:
///
/// - tab indention: Printer indents the expression with two tabs because the `align` increases the indention level.
/// - space indention: Printer indents the expression by 4 spaces (one indention level) **and** 2 spaces for the align.
pub fn align<Content, Context>(count: u8, content: &Content) -> Align<Context>
where
    Content: Format<Context>,
{
    Align {
        count: NonZeroU8::new(count).expect("Alignment count must be a non-zero number."),
        content: Argument::new(content),
    }
}

#[derive(Copy, Clone)]
pub struct Align<'a, Context> {
    count: NonZeroU8,
    content: Argument<'a, Context>,
}

impl<Context> Format<Context> for Align<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartAlign(tag::Align(self.count))))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndAlign))
    }
}

impl<Context> std::fmt::Debug for Align<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Align")
            .field("count", &self.count)
            .field("content", &"{{content}}")
            .finish()
    }
}

/// Inserts a hard line break before and after the content and increases the indention level for the content by one.
///
/// Block indents indent a block of code, such as in a function body, and therefore insert a line
/// break before and after the content.
///
/// Doesn't create an indention if the passed in content is [FormatElement.is_empty].
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format![
///     SimpleFormatContext::default(),
///     [
///         text("{"),
///         block_indent(&format_args![
///             text("let a = 10;"),
///             hard_line_break(),
///             text("let c = a + 5;"),
///         ]),
///         text("}"),
///     ]
/// ]?;
///
/// assert_eq!(
///     "{\n\tlet a = 10;\n\tlet c = a + 5;\n}",
///     block.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn block_indent<Context>(content: &impl Format<Context>) -> BlockIndent<Context> {
    BlockIndent {
        content: Argument::new(content),
        mode: IndentMode::Block,
    }
}

/// Indents the content by inserting a line break before and after the content and increasing
/// the indention level for the content by one if the enclosing group doesn't fit on a single line.
/// Doesn't change the formatting if the enclosing group fits on a single line.
///
/// # Examples
///
/// Indents the content by one level and puts in new lines if the enclosing `Group` doesn't fit on a single line
///
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(10).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("'First string',"),
///             soft_line_break_or_space(),
///             text("'second string',"),
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[\n\t'First string',\n\t'second string',\n]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// Doesn't change the formatting if the enclosing `Group` fits on a single line
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("5,"),
///             soft_line_break_or_space(),
///             text("10"),
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[5, 10]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn soft_block_indent<Context>(content: &impl Format<Context>) -> BlockIndent<Context> {
    BlockIndent {
        content: Argument::new(content),
        mode: IndentMode::Soft,
    }
}

/// If the enclosing `Group` doesn't fit on a single line, inserts a line break and indent.
/// Otherwise, just inserts a space.
///
/// Line indents are used to break a single line of code, and therefore only insert a line
/// break before the content and not after the content.
///
/// # Examples
///
/// Indents the content by one level and puts in new lines if the enclosing `Group` doesn't
/// fit on a single line. Otherwise, just inserts a space.
///
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(10).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("name"),
///         space(),
///         text("="),
///         soft_line_indent_or_space(&format_args![
///             text("firstName"),
///             space(),
///             text("+"),
///             space(),
///             text("lastName"),
///         ]),
///     ])
/// ])?;
///
/// assert_eq!(
///     "name =\n\tfirstName + lastName",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// Only adds a space if the enclosing `Group` fits on a single line
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("a"),
///         space(),
///         text("="),
///         soft_line_indent_or_space(&text("10")),
///     ])
/// ])?;
///
/// assert_eq!(
///     "a = 10",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn soft_line_indent_or_space<Context>(content: &impl Format<Context>) -> BlockIndent<Context> {
    BlockIndent {
        content: Argument::new(content),
        mode: IndentMode::SoftLineOrSpace,
    }
}

#[derive(Copy, Clone)]
pub struct BlockIndent<'a, Context> {
    content: Argument<'a, Context>,
    mode: IndentMode,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum IndentMode {
    Soft,
    Block,
    SoftSpace,
    SoftLineOrSpace,
}

impl<Context> Format<Context> for BlockIndent<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let snapshot = f.snapshot();

        f.write_element(FormatElement::Tag(StartIndent))?;

        match self.mode {
            IndentMode::Soft => write!(f, [soft_line_break()])?,
            IndentMode::Block => write!(f, [hard_line_break()])?,
            IndentMode::SoftLineOrSpace | IndentMode::SoftSpace => {
                write!(f, [soft_line_break_or_space()])?
            }
        }

        let is_empty = {
            let mut recording = f.start_recording();
            recording.write_fmt(Arguments::from(&self.content))?;
            recording.stop().is_empty()
        };

        if is_empty {
            f.restore_snapshot(snapshot);
            return Ok(());
        }

        f.write_element(FormatElement::Tag(EndIndent))?;

        match self.mode {
            IndentMode::Soft => write!(f, [soft_line_break()]),
            IndentMode::Block => write!(f, [hard_line_break()]),
            IndentMode::SoftSpace => write!(f, [soft_line_break_or_space()]),
            IndentMode::SoftLineOrSpace => Ok(()),
        }
    }
}

impl<Context> std::fmt::Debug for BlockIndent<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.mode {
            IndentMode::Soft => "SoftBlockIndent",
            IndentMode::Block => "HardBlockIndent",
            IndentMode::SoftLineOrSpace => "SoftLineIndentOrSpace",
            IndentMode::SoftSpace => "SoftSpaceBlockIndent",
        };

        f.debug_tuple(name).field(&"{{content}}").finish()
    }
}

/// Adds spaces around the content if its enclosing group fits on a line, otherwise indents the content and separates it by line breaks.
///
/// # Examples
///
/// Adds line breaks and indents the content if the enclosing group doesn't fit on the line.
///
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(10).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("{"),
///         soft_space_or_block_indent(&format_args![
///             text("aPropertyThatExceeds"),
///             text(":"),
///             space(),
///             text("'line width'"),
///         ]),
///         text("}")
///     ])
/// ])?;
///
/// assert_eq!(
///     "{\n\taPropertyThatExceeds: 'line width'\n}",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// Adds spaces around the content if the group fits on the line
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("{"),
///         soft_space_or_block_indent(&format_args![
///             text("a"),
///             text(":"),
///             space(),
///             text("5"),
///         ]),
///         text("}")
///     ])
/// ])?;
///
/// assert_eq!(
///     "{ a: 5 }",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
pub fn soft_space_or_block_indent<Context>(content: &impl Format<Context>) -> BlockIndent<Context> {
    BlockIndent {
        content: Argument::new(content),
        mode: IndentMode::SoftSpace,
    }
}

/// Creates a logical `Group` around the content that should either consistently be printed on a single line
/// or broken across multiple lines.
///
/// The printer will try to print the content of the `Group` on a single line, ignoring all soft line breaks and
/// emitting spaces for soft line breaks or spaces. The printer tracks back if it isn't successful either
/// because it encountered a hard line break, or because printing the `Group` on a single line exceeds
/// the configured line width, and thus it must print all its content on multiple lines,
/// emitting line breaks for all line break kinds.
///
/// # Examples
///
/// `Group` that fits on a single line
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("1,"),
///             soft_line_break_or_space(),
///             text("2,"),
///             soft_line_break_or_space(),
///             text("3"),
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[1, 2, 3]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// The printer breaks the `Group` over multiple lines if its content doesn't fit on a single line
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(20).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("'Good morning! How are you today?',"),
///             soft_line_break_or_space(),
///             text("2,"),
///             soft_line_break_or_space(),
///             text("3"),
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[\n\t'Good morning! How are you today?',\n\t2,\n\t3\n]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn group<Context>(content: &impl Format<Context>) -> Group<Context> {
    Group {
        content: Argument::new(content),
        group_id: None,
        should_expand: false,
    }
}

#[derive(Copy, Clone)]
pub struct Group<'a, Context> {
    content: Argument<'a, Context>,
    group_id: Option<GroupId>,
    should_expand: bool,
}

impl<Context> Group<'_, Context> {
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.group_id = group_id;
        self
    }

    /// Changes the [PrintMode] of the group from [`Flat`](PrintMode::Flat) to [`Expanded`](PrintMode::Expanded).
    /// The result is that any soft-line break gets printed as a regular line break.
    ///
    /// This is useful for content rendered inside of a [FormatElement::BestFitting] that prints each variant
    /// in [PrintMode::Flat] to change some content to be printed in [`Expanded`](PrintMode::Expanded) regardless.
    /// See the documentation of the [`best_fitting`] macro for an example.
    pub fn should_expand(mut self, should_expand: bool) -> Self {
        self.should_expand = should_expand;
        self
    }
}

impl<Context> Format<Context> for Group<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let mode = match self.should_expand {
            true => GroupMode::Expand,
            false => GroupMode::Flat,
        };

        f.write_element(FormatElement::Tag(StartGroup(
            tag::Group::new().with_id(self.group_id).with_mode(mode),
        )))?;

        Arguments::from(&self.content).fmt(f)?;

        f.write_element(FormatElement::Tag(EndGroup))
    }
}

impl<Context> std::fmt::Debug for Group<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupElements")
            .field("group_id", &self.group_id)
            .field("should_expand", &self.should_expand)
            .field("content", &"{{content}}")
            .finish()
    }
}

/// IR element that forces the parent group to print in expanded mode.
///
/// Has no effect if used outside of a group or element that introduce implicit groups (fill element).
///
/// ## Examples
///
/// ```
/// use ruff_formatter::{format, format_args, LineWidth};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("'Good morning! How are you today?',"),
///             soft_line_break_or_space(),
///             text("2,"),
///             expand_parent(), // Forces the parent to expand
///             soft_line_break_or_space(),
///             text("3"),
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[\n\t'Good morning! How are you today?',\n\t2,\n\t3\n]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// # Prettier
/// Equivalent to Prettier's `break_parent` IR element
pub const fn expand_parent() -> ExpandParent {
    ExpandParent
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ExpandParent;

impl<Context> Format<Context> for ExpandParent {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::ExpandParent)
    }
}

/// Adds a conditional content that is emitted only if it isn't inside an enclosing `Group` that
/// is printed on a single line. The element allows, for example, to insert a trailing comma after the last
/// array element only if the array doesn't fit on a single line.
///
/// The element has no special meaning if used outside of a `Group`. In that case, the content is always emitted.
///
/// If you're looking for a way to only print something if the `Group` fits on a single line see [self::if_group_fits_on_line].
///
/// # Examples
///
/// Omits the trailing comma for the last array element if the `Group` fits on a single line
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("1,"),
///             soft_line_break_or_space(),
///             text("2,"),
///             soft_line_break_or_space(),
///             text("3"),
///             if_group_breaks(&text(","))
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[1, 2, 3]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// Prints the trailing comma for the last array element if the `Group` doesn't fit on a single line
/// ```
/// use ruff_formatter::{format_args, format, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::printer::PrintWidth;
///
/// fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(20).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("'A somewhat longer string to force a line break',"),
///             soft_line_break_or_space(),
///             text("2,"),
///             soft_line_break_or_space(),
///             text("3"),
///             if_group_breaks(&text(","))
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[\n\t'A somewhat longer string to force a line break',\n\t2,\n\t3,\n]",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn if_group_breaks<Content, Context>(content: &Content) -> IfGroupBreaks<Context>
where
    Content: Format<Context>,
{
    IfGroupBreaks {
        content: Argument::new(content),
        group_id: None,
        mode: PrintMode::Expanded,
    }
}

/// Adds a conditional content specific for `Group`s that fit on a single line. The content isn't
/// emitted for `Group`s spanning multiple lines.
///
/// See [if_group_breaks] if you're looking for a way to print content only for groups spanning multiple lines.
///
/// # Examples
///
/// Adds the trailing comma for the last array element if the `Group` fits on a single line
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("1,"),
///             soft_line_break_or_space(),
///             text("2,"),
///             soft_line_break_or_space(),
///             text("3"),
///             if_group_fits_on_line(&text(","))
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[1, 2, 3,]",
///     formatted.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// Omits the trailing comma for the last array element if the `Group` doesn't fit on a single line
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(20).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let formatted = format!(context, [
///     group(&format_args![
///         text("["),
///         soft_block_indent(&format_args![
///             text("'A somewhat longer string to force a line break',"),
///             soft_line_break_or_space(),
///             text("2,"),
///             soft_line_break_or_space(),
///             text("3"),
///             if_group_fits_on_line(&text(","))
///         ]),
///         text("]"),
///     ])
/// ])?;
///
/// assert_eq!(
///     "[\n\t'A somewhat longer string to force a line break',\n\t2,\n\t3\n]",
///     formatted.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn if_group_fits_on_line<Content, Context>(flat_content: &Content) -> IfGroupBreaks<Context>
where
    Content: Format<Context>,
{
    IfGroupBreaks {
        mode: PrintMode::Flat,
        group_id: None,
        content: Argument::new(flat_content),
    }
}

#[derive(Copy, Clone)]
pub struct IfGroupBreaks<'a, Context> {
    content: Argument<'a, Context>,
    group_id: Option<GroupId>,
    mode: PrintMode,
}

impl<Context> IfGroupBreaks<'_, Context> {
    /// Inserts some content that the printer only prints if the group with the specified `group_id`
    /// is printed in multiline mode. The referred group must appear before this element in the document
    /// but doesn't have to one of its ancestors.
    ///
    /// # Examples
    ///
    /// Prints the trailing comma if the array group doesn't fit. The `group_id` is necessary
    /// because `fill` creates an implicit group around each item and tries to print the item in flat mode.
    /// The item `[4]` in this example fits on a single line but the trailing comma should still be printed
    ///
    /// ```
    /// use ruff_formatter::{format, format_args, write, LineWidth, SimpleFormatOptions};
    /// use ruff_formatter::prelude::*;
    ///
    /// # fn main() -> FormatResult<()> {
    /// let context = SimpleFormatContext::new(SimpleFormatOptions {
    ///     line_width: LineWidth::try_from(20).unwrap(),
    ///     ..SimpleFormatOptions::default()
    /// });
    ///
    /// let formatted = format!(context, [format_with(|f| {
    ///     let group_id = f.group_id("array");
    ///
    ///     write!(f, [
    ///         group(
    ///             &format_args![
    ///                 text("["),
    ///                 soft_block_indent(&format_with(|f| {
    ///                     f.fill()
    ///                         .entry(&soft_line_break_or_space(), &text("1,"))
    ///                         .entry(&soft_line_break_or_space(), &text("234568789,"))
    ///                         .entry(&soft_line_break_or_space(), &text("3456789,"))
    ///                         .entry(&soft_line_break_or_space(), &format_args!(
    ///                             text("["),
    ///                             soft_block_indent(&text("4")),
    ///                             text("]"),
    ///                             if_group_breaks(&text(",")).with_group_id(Some(group_id))
    ///                         ))
    ///                     .finish()
    ///                 })),
    ///                 text("]")
    ///             ],
    ///         ).with_group_id(Some(group_id))
    ///     ])
    /// })])?;
    ///
    /// assert_eq!(
    ///     "[\n\t1, 234568789,\n\t3456789, [4],\n]",
    ///     formatted.print()?.as_code()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.group_id = group_id;
        self
    }
}

impl<Context> Format<Context> for IfGroupBreaks<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartConditionalContent(
            Condition::new(self.mode).with_group_id(self.group_id),
        )))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndConditionalContent))
    }
}

impl<Context> std::fmt::Debug for IfGroupBreaks<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.mode {
            PrintMode::Flat => "IfGroupFitsOnLine",
            PrintMode::Expanded => "IfGroupBreaks",
        };

        f.debug_struct(name)
            .field("group_id", &self.group_id)
            .field("content", &"{{content}}")
            .finish()
    }
}

/// Increases the indent level by one if the group with the specified id breaks.
///
/// This IR has the same semantics as using [if_group_breaks] and [if_group_fits_on_line] together.
///
/// ```
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::write;
/// # let format = format_with(|f: &mut Formatter<SimpleFormatContext>| {
/// let id = f.group_id("head");
///
/// write!(f, [
///     group(&text("Head")).with_group_id(Some(id)),
///     if_group_breaks(&indent(&text("indented"))).with_group_id(Some(id)),
///     if_group_fits_on_line(&text("indented")).with_group_id(Some(id))
/// ])
///
/// # });
/// ```
///
/// If you want to indent some content if the enclosing group breaks, use [`indent`].
///
/// Use [if_group_breaks] or [if_group_fits_on_line] if the fitting and breaking content differs more than just the
/// indention level.
///
/// # Examples
///
/// Indent the body of an arrow function if the group wrapping the signature breaks:
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions, write};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let content = format_with(|f| {
///     let group_id = f.group_id("header");
///
///     write!(f, [
///         group(&text("(aLongHeaderThatBreaksForSomeReason) =>")).with_group_id(Some(group_id)),
///         indent_if_group_breaks(&format_args![hard_line_break(), text("a => b")], group_id)
///     ])
/// });
///
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(20).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let formatted = format!(context, [content])?;
///
/// assert_eq!(
///     "(aLongHeaderThatBreaksForSomeReason) =>\n\ta => b",
///     formatted.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
///
/// It doesn't add an indent if the group wrapping the signature doesn't break:
/// ```
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions, write};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let content = format_with(|f| {
///     let group_id = f.group_id("header");
///
///     write!(f, [
///         group(&text("(aLongHeaderThatBreaksForSomeReason) =>")).with_group_id(Some(group_id)),
///         indent_if_group_breaks(&format_args![hard_line_break(), text("a => b")], group_id)
///     ])
/// });
///
/// let formatted = format!(SimpleFormatContext::default(), [content])?;
///
/// assert_eq!(
///     "(aLongHeaderThatBreaksForSomeReason) =>\na => b",
///     formatted.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn indent_if_group_breaks<Content, Context>(
    content: &Content,
    group_id: GroupId,
) -> IndentIfGroupBreaks<Context>
where
    Content: Format<Context>,
{
    IndentIfGroupBreaks {
        group_id,
        content: Argument::new(content),
    }
}

#[derive(Copy, Clone)]
pub struct IndentIfGroupBreaks<'a, Context> {
    content: Argument<'a, Context>,
    group_id: GroupId,
}

impl<Context> Format<Context> for IndentIfGroupBreaks<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartIndentIfGroupBreaks(self.group_id)))?;
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndIndentIfGroupBreaks))
    }
}

impl<Context> std::fmt::Debug for IndentIfGroupBreaks<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndentIfGroupBreaks")
            .field("group_id", &self.group_id)
            .field("content", &"{{content}}")
            .finish()
    }
}

/// Utility for formatting some content with an inline lambda function.
#[derive(Copy, Clone)]
pub struct FormatWith<Context, T> {
    formatter: T,
    context: PhantomData<Context>,
}

impl<Context, T> Format<Context> for FormatWith<Context, T>
where
    T: Fn(&mut Formatter<Context>) -> FormatResult<()>,
{
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        (self.formatter)(f)
    }
}

impl<Context, T> std::fmt::Debug for FormatWith<Context, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FormatWith").field(&"{{formatter}}").finish()
    }
}

/// Creates an object implementing `Format` that calls the passed closure to perform the formatting.
///
/// # Examples
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{SimpleFormatContext, format, write};
/// use ruff_text_size::TextSize;
///
/// struct MyFormat {
///     items: Vec<&'static str>,
/// }
///
/// impl Format<SimpleFormatContext> for MyFormat {
///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
///         write!(f, [
///             text("("),
///             block_indent(&format_with(|f| {
///                 let separator = space();
///                 let mut join = f.join_with(&separator);
///
///                 for item in &self.items {
///                     join.entry(&format_with(|f| write!(f, [dynamic_text(item, None)])));
///                 }
///                 join.finish()
///             })),
///             text(")")
///         ])
///     }
/// }
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [MyFormat { items: vec!["a", "b", "c"]}])?;
///
/// assert_eq!("(\n\ta b c\n)", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
pub const fn format_with<Context, T>(formatter: T) -> FormatWith<Context, T>
where
    T: Fn(&mut Formatter<Context>) -> FormatResult<()>,
{
    FormatWith {
        formatter,
        context: PhantomData,
    }
}

/// Creates an inline `Format` object that can only be formatted once.
///
/// This can be useful in situation where the borrow checker doesn't allow you to use [`format_with`]
/// because the code formatting the content consumes the value and cloning the value is too expensive.
/// An example of this is if you want to nest a `FormatElement` or non-cloneable `Iterator` inside of a
/// `block_indent` as shown can see in the examples section.
///
/// # Panics
///
/// Panics if the object gets formatted more than once.
///
/// # Example
///
/// ```
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{SimpleFormatContext, format, write, Buffer};
///
/// struct MyFormat;
///
/// fn generate_values() -> impl Iterator<Item=StaticText> {
///     vec![text("1"), text("2"), text("3"), text("4")].into_iter()
/// }
///
/// impl Format<SimpleFormatContext> for MyFormat {
///     fn fmt(&self, f: &mut Formatter<SimpleFormatContext>) -> FormatResult<()> {
///         let mut values = generate_values();
///
///         let first = values.next();
///
///         // Formats the first item outside of the block and all other items inside of the block,
///         // separated by line breaks
///         write!(f, [
///             first,
///             block_indent(&format_once(|f| {
///                 // Using format_with isn't possible here because the iterator gets consumed here
///                 f.join_with(&hard_line_break()).entries(values).finish()
///             })),
///         ])
///     }
/// }
///
/// # fn main() -> FormatResult<()> {
/// let formatted = format!(SimpleFormatContext::default(), [MyFormat])?;
///
/// assert_eq!("1\n\t2\n\t3\n\t4\n", formatted.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
/// Formatting the same value twice results in a panic.
///
/// ```panics
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{SimpleFormatContext, format, write, Buffer};
/// use ruff_text_size::TextSize;
///
/// let mut count = 0;
///
/// let value = format_once(|f| {
///     write!(f, [dynamic_token(&std::format!("Formatted {count}."), TextSize::default())])
/// });
///
/// format!(SimpleFormatContext::default(), [value]).expect("Formatting once works fine");
///
/// // Formatting the value more than once panics
/// format!(SimpleFormatContext::default(), [value]);
/// ```
pub const fn format_once<T, Context>(formatter: T) -> FormatOnce<T, Context>
where
    T: FnOnce(&mut Formatter<Context>) -> FormatResult<()>,
{
    FormatOnce {
        formatter: Cell::new(Some(formatter)),
        context: PhantomData,
    }
}

pub struct FormatOnce<T, Context> {
    formatter: Cell<Option<T>>,
    context: PhantomData<Context>,
}

impl<T, Context> Format<Context> for FormatOnce<T, Context>
where
    T: FnOnce(&mut Formatter<Context>) -> FormatResult<()>,
{
    #[inline(always)]
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let formatter = self.formatter.take().expect("Tried to format a `format_once` at least twice. This is not allowed. You may want to use `format_with` or `format.memoized` instead.");

        (formatter)(f)
    }
}

impl<T, Context> std::fmt::Debug for FormatOnce<T, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FormatOnce").field(&"{{formatter}}").finish()
    }
}

/// Builder to join together a sequence of content.
/// See [Formatter::join]
#[must_use = "must eventually call `finish()` on Format builders"]
pub struct JoinBuilder<'fmt, 'buf, Separator, Context> {
    result: FormatResult<()>,
    fmt: &'fmt mut Formatter<'buf, Context>,
    with: Option<Separator>,
    has_elements: bool,
}

impl<'fmt, 'buf, Separator, Context> JoinBuilder<'fmt, 'buf, Separator, Context>
where
    Separator: Format<Context>,
{
    /// Creates a new instance that joins the elements without a separator
    pub(super) fn new(fmt: &'fmt mut Formatter<'buf, Context>) -> Self {
        Self {
            result: Ok(()),
            fmt,
            has_elements: false,
            with: None,
        }
    }

    /// Creates a new instance that prints the passed separator between every two entries.
    pub(super) fn with_separator(fmt: &'fmt mut Formatter<'buf, Context>, with: Separator) -> Self {
        Self {
            result: Ok(()),
            fmt,
            has_elements: false,
            with: Some(with),
        }
    }

    /// Adds a new entry to the join output.
    pub fn entry(&mut self, entry: &dyn Format<Context>) -> &mut Self {
        self.result = self.result.and_then(|_| {
            if let Some(with) = &self.with {
                if self.has_elements {
                    with.fmt(self.fmt)?;
                }
            }
            self.has_elements = true;

            entry.fmt(self.fmt)
        });

        self
    }

    /// Adds the contents of an iterator of entries to the join output.
    pub fn entries<F, I>(&mut self, entries: I) -> &mut Self
    where
        F: Format<Context>,
        I: IntoIterator<Item = F>,
    {
        for entry in entries {
            self.entry(&entry);
        }

        self
    }

    /// Finishes the output and returns any error encountered.
    pub fn finish(&mut self) -> FormatResult<()> {
        self.result
    }
}

/// Builder to fill as many elements as possible on a single line.
#[must_use = "must eventually call `finish()` on Format builders"]
pub struct FillBuilder<'fmt, 'buf, Context> {
    result: FormatResult<()>,
    fmt: &'fmt mut Formatter<'buf, Context>,
    empty: bool,
}

impl<'a, 'buf, Context> FillBuilder<'a, 'buf, Context> {
    pub(crate) fn new(fmt: &'a mut Formatter<'buf, Context>) -> Self {
        let result = fmt.write_element(FormatElement::Tag(StartFill));

        Self {
            result,
            fmt,
            empty: true,
        }
    }

    /// Adds an iterator of entries to the fill output. Uses the passed `separator` to separate any two items.
    pub fn entries<F, I>(&mut self, separator: &dyn Format<Context>, entries: I) -> &mut Self
    where
        F: Format<Context>,
        I: IntoIterator<Item = F>,
    {
        for entry in entries {
            self.entry(separator, &entry);
        }

        self
    }

    /// Adds a new entry to the fill output. The `separator` isn't written if this is the first element in the list.
    pub fn entry(
        &mut self,
        separator: &dyn Format<Context>,
        entry: &dyn Format<Context>,
    ) -> &mut Self {
        self.result = self.result.and_then(|_| {
            if self.empty {
                self.empty = false;
            } else {
                self.fmt.write_element(FormatElement::Tag(StartEntry))?;
                separator.fmt(self.fmt)?;
                self.fmt.write_element(FormatElement::Tag(EndEntry))?;
            }

            self.fmt.write_element(FormatElement::Tag(StartEntry))?;
            entry.fmt(self.fmt)?;
            self.fmt.write_element(FormatElement::Tag(EndEntry))
        });

        self
    }

    /// Finishes the output and returns any error encountered
    pub fn finish(&mut self) -> FormatResult<()> {
        self.result
            .and_then(|_| self.fmt.write_element(FormatElement::Tag(EndFill)))
    }
}

/// The first variant is the most flat, and the last is the most expanded variant.
/// See [`best_fitting!`] macro for a more in-detail documentation
#[derive(Copy, Clone)]
pub struct BestFitting<'a, Context> {
    variants: Arguments<'a, Context>,
    mode: BestFittingMode,
}

impl<'a, Context> BestFitting<'a, Context> {
    /// Creates a new best fitting IR with the given variants. The method itself isn't unsafe
    /// but it is to discourage people from using it because the printer will panic if
    /// the slice doesn't contain at least the least and most expanded variants.
    ///
    /// You're looking for a way to create a `BestFitting` object, use the `best_fitting![least_expanded, most_expanded]` macro.
    ///
    /// ## Safety
    /// The slice must contain at least two variants.
    pub unsafe fn from_arguments_unchecked(variants: Arguments<'a, Context>) -> Self {
        assert!(
            variants.0.len() >= 2,
            "Requires at least the least expanded and most expanded variants"
        );

        Self {
            variants,
            mode: BestFittingMode::default(),
        }
    }

    /// Changes the mode used by this best fitting element to determine whether a variant fits.
    ///
    /// ## Examples
    ///
    /// ### All Lines
    ///
    /// ```
    /// use ruff_formatter::{Formatted, LineWidth, format, format_args, SimpleFormatOptions};
    /// use ruff_formatter::prelude::*;
    ///
    /// # fn main() -> FormatResult<()> {
    /// let formatted = format!(
    ///     SimpleFormatContext::default(),
    ///     [
    ///         best_fitting!(
    ///             // Everything fits on a single line
    ///             format_args!(
    ///                 group(&format_args![
    ///                     text("["),
    ///                         soft_block_indent(&format_args![
    ///                         text("1,"),
    ///                         soft_line_break_or_space(),
    ///                         text("2,"),
    ///                         soft_line_break_or_space(),
    ///                         text("3"),
    ///                     ]),
    ///                     text("]")
    ///                 ]),
    ///                 space(),
    ///                 text("+"),
    ///                 space(),
    ///                 text("aVeryLongIdentifier")
    ///             ),
    ///
    ///             // Breaks after `[` and prints each elements on a single line
    ///             // The group is necessary because the variant, by default is printed in flat mode and a
    ///             // hard line break indicates that the content doesn't fit.
    ///             format_args!(
    ///                 text("["),
    ///                 group(&block_indent(&format_args![text("1,"), hard_line_break(), text("2,"), hard_line_break(), text("3")])).should_expand(true),
    ///                 text("]"),
    ///                 space(),
    ///                 text("+"),
    ///                 space(),
    ///                 text("aVeryLongIdentifier")
    ///             ),
    ///
    ///             // Adds parentheses and indents the body, breaks after the operator
    ///             format_args!(
    ///                 text("("),
    ///                 block_indent(&format_args![
    ///                     text("["),
    ///                     block_indent(&format_args![
    ///                         text("1,"),
    ///                         hard_line_break(),
    ///                         text("2,"),
    ///                         hard_line_break(),
    ///                         text("3"),
    ///                     ]),
    ///                     text("]"),
    ///                     hard_line_break(),
    ///                     text("+"),
    ///                     space(),
    ///                     text("aVeryLongIdentifier")
    ///                 ]),
    ///                 text(")")
    ///             )
    ///         ).with_mode(BestFittingMode::AllLines)
    ///     ]
    /// )?;
    ///
    /// let document = formatted.into_document();
    ///
    /// // Takes the first variant if everything fits on a single line
    /// assert_eq!(
    ///     "[1, 2, 3] + aVeryLongIdentifier",
    ///     Formatted::new(document.clone(), SimpleFormatContext::default())
    ///         .print()?
    ///         .as_code()
    /// );
    ///
    /// // It takes the second if the first variant doesn't fit on a single line. The second variant
    /// // has some additional line breaks to make sure inner groups don't break
    /// assert_eq!(
    ///     "[\n\t1,\n\t2,\n\t3\n] + aVeryLongIdentifier",
    ///     Formatted::new(document.clone(), SimpleFormatContext::new(SimpleFormatOptions { line_width: 23.try_into().unwrap(), ..SimpleFormatOptions::default() }))
    ///         .print()?
    ///         .as_code()
    /// );
    ///
    /// // Prints the last option as last resort
    /// assert_eq!(
    ///     "(\n\t[\n\t\t1,\n\t\t2,\n\t\t3\n\t]\n\t+ aVeryLongIdentifier\n)",
    ///     Formatted::new(document.clone(), SimpleFormatContext::new(SimpleFormatOptions { line_width: 22.try_into().unwrap(), ..SimpleFormatOptions::default() }))
    ///         .print()?
    ///         .as_code()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_mode(mut self, mode: BestFittingMode) -> Self {
        self.mode = mode;
        self
    }
}

impl<Context> Format<Context> for BestFitting<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let mut buffer = VecBuffer::new(f.state_mut());
        let variants = self.variants.items();

        let mut formatted_variants = Vec::with_capacity(variants.len());

        for variant in variants {
            buffer.write_element(FormatElement::Tag(StartEntry))?;
            buffer.write_fmt(Arguments::from(variant))?;
            buffer.write_element(FormatElement::Tag(EndEntry))?;

            formatted_variants.push(buffer.take_vec().into_boxed_slice());
        }

        // SAFETY: The constructor guarantees that there are always at least two variants. It's, therefore,
        // safe to call into the unsafe `from_vec_unchecked` function
        let element = unsafe {
            FormatElement::BestFitting {
                variants: format_element::BestFittingVariants::from_vec_unchecked(
                    formatted_variants,
                ),
                mode: self.mode,
            }
        };

        f.write_element(element)
    }
}
