use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroU8;

use ruff_text_size::TextRange;
#[allow(clippy::enum_glob_use)]
use Tag::*;

use crate::format_element::tag::{Condition, Tag};
use crate::prelude::tag::{DedentMode, GroupMode, LabelId};
use crate::prelude::*;
use crate::{write, Argument, Arguments, FormatContext, FormatOptions, GroupId, TextSize};
use crate::{Buffer, VecBuffer};

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
///     group(&format_args![token("a,"), soft_line_break(), token("b")])
/// ])?;
///
/// assert_eq!(
///     "a,b",
///     elements.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
/// See [`soft_line_break_or_space`] if you want to insert a space between the elements if the enclosing
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
///         token("a long word,"),
///         soft_line_break(),
///         token("so that the group doesn't fit on a single line"),
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
///         token("a,"),
///         hard_line_break(),
///         token("b"),
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
///         token("a,"),
///         empty_line(),
///         token("b"),
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
/// The line breaks are emitted as spaces if the enclosing `Group` fits on a single line:
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     group(&format_args![
///         token("a,"),
///         soft_line_break_or_space(),
///         token("b"),
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
///         token("a long word,"),
///         soft_line_break_or_space(),
///         token("so that the group doesn't fit on a single line"),
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
        f.write_element(FormatElement::Line(self.mode));
        Ok(())
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Line").field(&self.mode).finish()
    }
}

/// Creates a token that gets written as is to the output. A token must be ASCII only and is not allowed
/// to contain any line breaks or tab characters.
///
/// # Examples
///
/// ```
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [token("Hello World")])?;
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
/// let elements = format!(SimpleFormatContext::default(), [token("\"Hello\\tWorld\"")])?;
///
/// assert_eq!(r#""Hello\tWorld""#, elements.print()?.as_code());
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn token(text: &'static str) -> Token {
    debug_assert!(text.is_ascii(), "Token must be ASCII text only");
    debug_assert!(
        !text.contains(['\n', '\r', '\t']),
        "A token should not contain any newlines or tab characters"
    );

    Token { text }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Token {
    text: &'static str,
}

impl<Context> Format<Context> for Token {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Token { text: self.text });
        Ok(())
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::write!(f, "Token({})", self.text)
    }
}

/// Creates a source map entry from the passed source `position` to the position in the formatted output.
///
/// ## Examples
///
/// ```
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
///     token("\"Hello "),
///     source_position(TextSize::new(8)),
///     token("'Ruff'"),
///     source_position(TextSize::new(14)),
///     token("\""),
///     source_position(TextSize::new(20))
/// ])?;
///
/// let printed = elements.print()?;
///
/// assert_eq!(printed.as_code(), r#""Hello 'Ruff'""#);
/// assert_eq!(printed.sourcemap(), [
///     SourceMarker { source: TextSize::new(0), dest: TextSize::new(0) },
///     SourceMarker { source: TextSize::new(8), dest: TextSize::new(7) },
///     SourceMarker { source: TextSize::new(14), dest: TextSize::new(13) },
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
        if let Some(FormatElement::SourcePosition(last_position)) = f.buffer.elements().last() {
            if *last_position == self.0 {
                return Ok(());
            }
        }

        f.write_element(FormatElement::SourcePosition(self.0));

        Ok(())
    }
}

/// Creates a text from a dynamic string.
///
/// This is done by allocating a new string internally.
pub fn text(text: &str) -> Text {
    debug_assert_no_newlines(text);

    Text { text }
}

#[derive(Eq, PartialEq)]
pub struct Text<'a> {
    text: &'a str,
}

impl<Context> Format<Context> for Text<'_>
where
    Context: FormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Text {
            text: self.text.to_string().into_boxed_str(),
            text_width: TextWidth::from_text(self.text, f.options().indent_width()),
        });

        Ok(())
    }
}

impl std::fmt::Debug for Text<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::write!(f, "Text({})", self.text)
    }
}

/// Emits a text as it is written in the source document. Optimized to avoid allocations.
pub const fn source_text_slice(range: TextRange) -> SourceTextSliceBuilder {
    SourceTextSliceBuilder { range }
}

#[derive(Eq, PartialEq, Debug)]
pub struct SourceTextSliceBuilder {
    range: TextRange,
}

impl<Context> Format<Context> for SourceTextSliceBuilder
where
    Context: FormatContext,
{
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let source_code = f.context().source_code();
        let slice = source_code.slice(self.range);
        debug_assert_no_newlines(slice.text(source_code));

        let text_width = TextWidth::from_text(
            slice.text(source_code),
            f.context().options().indent_width(),
        );

        f.write_element(FormatElement::SourceCodeSlice { slice, text_width });

        Ok(())
    }
}

fn debug_assert_no_newlines(text: &str) {
    debug_assert!(!text.contains('\r'), "The content '{text}' contains an unsupported '\\r' line terminator character but text must only use line feeds '\\n' as line separator. Use '\\n' instead of '\\r' and '\\r\\n' to insert a line break in strings.");
}

/// Pushes some content to the end of the current line.
///
/// ## Examples
///
/// ```rust
/// use ruff_formatter::format;
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let elements = format!(SimpleFormatContext::default(), [
///     token("a"),
///     line_suffix(&token("c"), 0),
///     token("b")
/// ])?;
///
/// assert_eq!("abc", elements.print()?.as_code());
/// # Ok(())
/// # }
/// ```
///
/// Provide reserved width for the line suffix to include it during measurement.
/// ```rust
/// use ruff_formatter::{format, format_args, LineWidth, SimpleFormatContext, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(10).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     // Breaks
///     group(&format_args![
///         if_group_breaks(&token("(")),
///         soft_block_indent(&format_args![token("a"), line_suffix(&token(" // a comment"), 13)]),
///         if_group_breaks(&token(")"))
///         ]),
///
///     // Fits
///     group(&format_args![
///         if_group_breaks(&token("(")),
///         soft_block_indent(&format_args![token("a"), line_suffix(&token(" // a comment"), 0)]),
///         if_group_breaks(&token(")"))
///     ]),
/// ])?;
/// # assert_eq!("(\n\ta // a comment\n)a // a comment", elements.print()?.as_code());
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn line_suffix<Content, Context>(inner: &Content, reserved_width: u32) -> LineSuffix<Context>
where
    Content: Format<Context>,
{
    LineSuffix {
        content: Argument::new(inner),
        reserved_width,
    }
}

#[derive(Copy, Clone)]
pub struct LineSuffix<'a, Context> {
    content: Argument<'a, Context>,
    reserved_width: u32,
}

impl<Context> Format<Context> for LineSuffix<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartLineSuffix {
            reserved_width: self.reserved_width,
        }));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndLineSuffix));

        Ok(())
    }
}

impl<Context> std::fmt::Debug for LineSuffix<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("LineSuffix").field(&"{{content}}").finish()
    }
}

/// Inserts a boundary for line suffixes that forces the printer to print all pending line suffixes.
/// Helpful if a line suffix shouldn't pass a certain point.
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
///     token("a"),
///     line_suffix(&token("c"), 0),
///     token("b"),
///     line_suffix_boundary(),
///     token("d")
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
        f.write_element(FormatElement::LineSuffixBoundary);

        Ok(())
    }
}

/// Marks some content with a label.
///
/// This does not directly influence how this content will be printed, but some
/// parts of the formatter may inspect the [labelled element](Tag::StartLabelled)
/// using [`FormatElements::has_label`].
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
///                 &token("'I have a label'")
///             )
///         ])?;
///
///         let recorded = recording.stop();
///
///         let is_labelled = recorded.first().is_some_and( |element| element.has_label(LabelId::of(MyLabels::Main)));
///
///         if is_labelled {
///             write!(f, [token(" has label `Main`")])
///         } else {
///             write!(f, [token(" doesn't have label `Main`")])
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
        f.write_element(FormatElement::Tag(StartLabelled(self.label_id)));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndLabelled));

        Ok(())
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
/// let elements = format!(SimpleFormatContext::default(), [token("a"), space(), token("b")])?;
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
        f.write_element(FormatElement::Space);

        Ok(())
    }
}

/// It adds a level of indentation to the given content
///
/// It doesn't add any line breaks at the edges of the content, meaning that
/// the line breaks have to be manually added.
///
/// This helper should be used only in rare cases, instead you should rely more on
/// [`block_indent`] and [`soft_block_indent`]
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     token("switch {"),
///     block_indent(&format_args![
///         token("default:"),
///         indent(&format_args![
///             // this is where we want to use a
///             hard_line_break(),
///             token("break;"),
///         ])
///     ]),
///     token("}"),
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
        f.write_element(FormatElement::Tag(StartIndent));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndIndent));

        Ok(())
    }
}

impl<Context> std::fmt::Debug for Indent<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Indent").field(&"{{content}}").finish()
    }
}

/// It reduces the indentation for the given content depending on the closest [indent] or [align] parent element.
/// - [align] Undoes the spaces added by [align]
/// - [indent] Reduces the indentation level by one
///
/// This is a No-op if the indentation level is zero.
///
/// # Examples
///
/// ```
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     token("root"),
///     align(2, &format_args![
///         hard_line_break(),
///         token("aligned"),
///         dedent(&format_args![
///             hard_line_break(),
///             token("not aligned"),
///         ]),
///         dedent(&indent(&format_args![
///             hard_line_break(),
///             token("Indented, not aligned")
///         ]))
///     ]),
///     dedent(&format_args![
///         hard_line_break(),
///         token("Dedent on root level is a no-op.")
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
        f.write_element(FormatElement::Tag(StartDedent(self.mode)));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndDedent));

        Ok(())
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
///     token("root"),
///     indent(&format_args![
///         hard_line_break(),
///         token("indent level 1"),
///         indent(&format_args![
///             hard_line_break(),
///             token("indent level 2"),
///             align(2, &format_args![
///                 hard_line_break(),
///                 token("two space align"),
///                 dedent_to_root(&format_args![
///                     hard_line_break(),
///                     token("starts at the beginning of the line")
///                 ]),
///             ]),
///             hard_line_break(),
///             token("end indent level 2"),
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
/// ## Tab indentation
///
/// ```
/// use std::num::NonZeroU8;
/// use ruff_formatter::{format, format_args};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let block = format!(SimpleFormatContext::default(), [
///     token("a"),
///     hard_line_break(),
///     token("?"),
///     space(),
///     align(2, &format_args![
///         token("function () {"),
///         hard_line_break(),
///         token("}"),
///     ]),
///     hard_line_break(),
///     token(":"),
///     space(),
///     align(2, &format_args![
///         token("function () {"),
///         block_indent(&token("console.log('test');")),
///         token("}"),
///     ]),
///     token(";")
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
///   This is because `align` increases the indentation level by one (same as `indent`)
///   if you nest an `indent` inside an `align`.
///   Meaning that, `align > ... > indent` results in the same indentation as `indent > ... > indent`.
///
/// ## Spaces indentation
///
/// ```
/// use std::num::NonZeroU8;
/// use ruff_formatter::{format, format_args, IndentStyle, SimpleFormatOptions};
/// use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// use ruff_formatter::IndentWidth;
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     indent_style: IndentStyle::Space,
///     indent_width: IndentWidth::try_from(4).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let block = format!(context, [
///     token("a"),
///     hard_line_break(),
///     token("?"),
///     space(),
///     align(2, &format_args![
///         token("function () {"),
///         hard_line_break(),
///         token("}"),
///     ]),
///     hard_line_break(),
///     token(":"),
///     space(),
///     align(2, &format_args![
///         token("function () {"),
///         block_indent(&token("console.log('test');")),
///         token("}"),
///     ]),
///     token(";")
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
/// The printing of `align` differs if using spaces as indentation sequence *and* it contains an `indent`.
/// You can see the difference when comparing the indentation of the `console.log(...)` expression to the previous example:
///
/// - tab indentation: Printer indents the expression with two tabs because the `align` increases the indentation level.
/// - space indentation: Printer indents the expression by 4 spaces (one indentation level) **and** 2 spaces for the align.
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
        f.write_element(FormatElement::Tag(StartAlign(tag::Align(self.count))));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndAlign));

        Ok(())
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

/// Inserts a hard line break before and after the content and increases the indentation level for the content by one.
///
/// Block indents indent a block of code, such as in a function body, and therefore insert a line
/// break before and after the content.
///
/// Doesn't create an indentation if the passed in content is [`FormatElement.is_empty`].
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
///         token("{"),
///         block_indent(&format_args![
///             token("let a = 10;"),
///             hard_line_break(),
///             token("let c = a + 5;"),
///         ]),
///         token("}"),
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
/// the indentation level for the content by one if the enclosing group doesn't fit on a single line.
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("'First string',"),
///             soft_line_break_or_space(),
///             token("'second string',"),
///         ]),
///         token("]"),
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("5,"),
///             soft_line_break_or_space(),
///             token("10"),
///         ]),
///         token("]"),
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
///         token("name"),
///         space(),
///         token("="),
///         soft_line_indent_or_space(&format_args![
///             token("firstName"),
///             space(),
///             token("+"),
///             space(),
///             token("lastName"),
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
///         token("a"),
///         space(),
///         token("="),
///         soft_line_indent_or_space(&token("10")),
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

        f.write_element(FormatElement::Tag(StartIndent));

        match self.mode {
            IndentMode::Soft => write!(f, [soft_line_break()])?,
            IndentMode::Block => write!(f, [hard_line_break()])?,
            IndentMode::SoftLineOrSpace | IndentMode::SoftSpace => {
                write!(f, [soft_line_break_or_space()])?;
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

        f.write_element(FormatElement::Tag(EndIndent));

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
///         token("{"),
///         soft_space_or_block_indent(&format_args![
///             token("aPropertyThatExceeds"),
///             token(":"),
///             space(),
///             token("'line width'"),
///         ]),
///         token("}")
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
///         token("{"),
///         soft_space_or_block_indent(&format_args![
///             token("a"),
///             token(":"),
///             space(),
///             token("5"),
///         ]),
///         token("}")
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("1,"),
///             soft_line_break_or_space(),
///             token("2,"),
///             soft_line_break_or_space(),
///             token("3"),
///         ]),
///         token("]"),
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("'Good morning! How are you today?',"),
///             soft_line_break_or_space(),
///             token("2,"),
///             soft_line_break_or_space(),
///             token("3"),
///         ]),
///         token("]"),
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
    #[must_use]
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.group_id = group_id;
        self
    }

    /// Changes the [`PrintMode`] of the group from [`Flat`](PrintMode::Flat) to [`Expanded`](PrintMode::Expanded).
    /// The result is that any soft-line break gets printed as a regular line break.
    ///
    /// This is useful for content rendered inside of a [`FormatElement::BestFitting`] that prints each variant
    /// in [`PrintMode::Flat`] to change some content to be printed in [`Expanded`](PrintMode::Expanded) regardless.
    /// See the documentation of the [`best_fitting`] macro for an example.
    #[must_use]
    pub fn should_expand(mut self, should_expand: bool) -> Self {
        self.should_expand = should_expand;
        self
    }
}

impl<Context> Format<Context> for Group<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let mode = if self.should_expand {
            GroupMode::Expand
        } else {
            GroupMode::Flat
        };

        f.write_element(FormatElement::Tag(StartGroup(
            tag::Group::new().with_id(self.group_id).with_mode(mode),
        )));

        Arguments::from(&self.content).fmt(f)?;

        f.write_element(FormatElement::Tag(EndGroup));

        Ok(())
    }
}

impl<Context> std::fmt::Debug for Group<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Group")
            .field("group_id", &self.group_id)
            .field("should_expand", &self.should_expand)
            .field("content", &"{{content}}")
            .finish()
    }
}

/// Content that may get parenthesized if it exceeds the configured line width but only if the parenthesized
/// layout doesn't exceed the line width too, in which case it falls back to the flat layout.
///
/// This IR is identical to the following [`best_fitting`] layout but is implemented as custom IR for
/// better performance.
///
/// ```rust
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::format_args;
///
/// let format_expression = format_with(|f: &mut Formatter<SimpleFormatContext>| token("A long string").fmt(f));
/// let _ = best_fitting![
///     // ---------------------------------------------------------------------
///     // Variant 1:
///     // Try to fit the expression without any parentheses
///     group(&format_expression),
///     // ---------------------------------------------------------------------
///     // Variant 2:
///     // Try to fit the expression by adding parentheses and indenting the expression.
///     group(&format_args![
///         token("("),
///         soft_block_indent(&format_expression),
///         token(")")
///     ])
///     .should_expand(true),
///     // ---------------------------------------------------------------------
///     // Variant 3: Fallback, no parentheses
///     // Expression doesn't fit regardless of adding the parentheses. Remove the parentheses again.
///     group(&format_expression).should_expand(true)
/// ]
/// // Measure all lines, to avoid that the printer decides that this fits right after hitting
/// // the `(`.
/// .with_mode(BestFittingMode::AllLines)        ;
/// ```
///
/// The element breaks from left-to-right because it uses the unintended version as *expanded* layout, the same as the above showed best fitting example.
///
/// ## Examples
///
/// ### Content that fits into the configured line width.
///
/// ```rust
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::{format, PrintResult, write};
///
/// # fn main() -> FormatResult<()> {
///     let formatted = format!(SimpleFormatContext::default(), [format_with(|f| {
///         write!(f, [
///             token("aLongerVariableName = "),
///             best_fit_parenthesize(&token("'a string that fits into the configured line width'"))
///         ])
///     })])?;
///
///     assert_eq!(formatted.print()?.as_code(), "aLongerVariableName = 'a string that fits into the configured line width'");
///     # Ok(())
/// # }
/// ```
///
/// ### Content that fits parenthesized
///
/// ```rust
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::{format, PrintResult, write};
///
/// # fn main() -> FormatResult<()> {
///     let formatted = format!(SimpleFormatContext::default(), [format_with(|f| {
///         write!(f, [
///             token("aLongerVariableName = "),
///             best_fit_parenthesize(&token("'a string that exceeds configured line width but fits parenthesized'"))
///         ])
///     })])?;
///
///     assert_eq!(formatted.print()?.as_code(), "aLongerVariableName = (\n\t'a string that exceeds configured line width but fits parenthesized'\n)");
///     # Ok(())
/// # }
/// ```
///
/// ### Content that exceeds the line width, parenthesized or not
///
/// ```rust
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::{format, PrintResult, write};
///
/// # fn main() -> FormatResult<()> {
///     let formatted = format!(SimpleFormatContext::default(), [format_with(|f| {
///         write!(f, [
///             token("aLongerVariableName = "),
///             best_fit_parenthesize(&token("'a string that exceeds the configured line width and even parenthesizing doesn't make it fit'"))
///         ])
///     })])?;
///
///     assert_eq!(formatted.print()?.as_code(), "aLongerVariableName = 'a string that exceeds the configured line width and even parenthesizing doesn't make it fit'");
///     # Ok(())
/// # }
/// ```
#[inline]
pub fn best_fit_parenthesize<Context>(
    content: &impl Format<Context>,
) -> BestFitParenthesize<Context> {
    BestFitParenthesize {
        content: Argument::new(content),
        group_id: None,
    }
}

#[derive(Copy, Clone)]
pub struct BestFitParenthesize<'a, Context> {
    content: Argument<'a, Context>,
    group_id: Option<GroupId>,
}

impl<Context> BestFitParenthesize<'_, Context> {
    /// Optional ID that can be used in conditional content that supports [`Condition`] to gate content
    /// depending on whether the parentheses are rendered (flat: no parentheses, expanded: parentheses).
    #[must_use]
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.group_id = group_id;
        self
    }
}

impl<Context> Format<Context> for BestFitParenthesize<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartBestFitParenthesize {
            id: self.group_id,
        }));

        Arguments::from(&self.content).fmt(f)?;

        f.write_element(FormatElement::Tag(EndBestFitParenthesize));

        Ok(())
    }
}

impl<Context> std::fmt::Debug for BestFitParenthesize<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BestFitParenthesize")
            .field("group_id", &self.group_id)
            .field("content", &"{{content}}")
            .finish()
    }
}

/// Sets the `condition` for the group. The element will behave as a regular group if `condition` is met,
/// and as *ungrouped* content if the condition is not met.
///
/// ## Examples
///
/// Only expand before operators if the parentheses are necessary.
///
/// ```
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions};
///
/// # fn main() -> FormatResult<()> {
/// use ruff_formatter::Formatted;
/// let content = format_with(|f| {
///     let parentheses_id = f.group_id("parentheses");
///     group(&format_args![
///         if_group_breaks(&token("(")),
///         indent_if_group_breaks(&format_args![
///             soft_line_break(),
///             conditional_group(&format_args![
///                 token("'aaaaaaa'"),
///                 soft_line_break_or_space(),
///                 token("+"),
///                 space(),
///                 fits_expanded(&conditional_group(&format_args![
///                     token("["),
///                     soft_block_indent(&format_args![
///                         token("'Good morning!',"),
///                         soft_line_break_or_space(),
///                         token("'How are you?'"),
///                     ]),
///                     token("]"),
///                 ], tag::Condition::if_group_fits_on_line(parentheses_id))),
///                 soft_line_break_or_space(),
///                 token("+"),
///                 space(),
///                 conditional_group(&format_args![
///                     token("'bbbb'"),
///                     soft_line_break_or_space(),
///                     token("and"),
///                     space(),
///                     token("'c'")
///                 ], tag::Condition::if_group_fits_on_line(parentheses_id))
///             ], tag::Condition::if_breaks()),
///         ], parentheses_id),
///         soft_line_break(),
///         if_group_breaks(&token(")"))
///     ])
///     .with_group_id(Some(parentheses_id))
///     .fmt(f)
/// });
///
/// let formatted = format!(SimpleFormatContext::default(), [content])?;
/// let document = formatted.into_document();
///
/// // All content fits
/// let all_fits = Formatted::new(document.clone(), SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(65).unwrap(),
///     ..SimpleFormatOptions::default()
/// }));
///
/// assert_eq!(
///     "'aaaaaaa' + ['Good morning!', 'How are you?'] + 'bbbb' and 'c'",
///     all_fits.print()?.as_code()
/// );
///
/// // The parentheses group fits, because it can expand the list,
/// let list_expanded = Formatted::new(document.clone(), SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(21).unwrap(),
///     ..SimpleFormatOptions::default()
/// }));
///
/// assert_eq!(
///     "'aaaaaaa' + [\n\t'Good morning!',\n\t'How are you?'\n] + 'bbbb' and 'c'",
///     list_expanded.print()?.as_code()
/// );
///
/// // It is necessary to split all groups to fit the content
/// let all_expanded = Formatted::new(document, SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(11).unwrap(),
///     ..SimpleFormatOptions::default()
/// }));
///
/// assert_eq!(
///     "(\n\t'aaaaaaa'\n\t+ [\n\t\t'Good morning!',\n\t\t'How are you?'\n\t]\n\t+ 'bbbb'\n\tand 'c'\n)",
///     all_expanded.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn conditional_group<Content, Context>(
    content: &Content,
    condition: Condition,
) -> ConditionalGroup<Context>
where
    Content: Format<Context>,
{
    ConditionalGroup {
        content: Argument::new(content),
        condition,
    }
}

#[derive(Clone)]
pub struct ConditionalGroup<'content, Context> {
    content: Argument<'content, Context>,
    condition: Condition,
}

impl<Context> Format<Context> for ConditionalGroup<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartConditionalGroup(
            tag::ConditionalGroup::new(self.condition),
        )));
        f.write_fmt(Arguments::from(&self.content))?;
        f.write_element(FormatElement::Tag(EndConditionalGroup));

        Ok(())
    }
}

impl<Context> std::fmt::Debug for ConditionalGroup<'_, Context> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConditionalGroup")
            .field("condition", &self.condition)
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("'Good morning! How are you today?',"),
///             soft_line_break_or_space(),
///             token("2,"),
///             expand_parent(), // Forces the parent to expand
///             soft_line_break_or_space(),
///             token("3"),
///         ]),
///         token("]"),
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
        f.write_element(FormatElement::ExpandParent);

        Ok(())
    }
}

/// Adds a conditional content that is emitted only if it isn't inside an enclosing `Group` that
/// is printed on a single line. The element allows, for example, to insert a trailing comma after the last
/// array element only if the array doesn't fit on a single line.
///
/// The element has no special meaning if used outside of a `Group`. In that case, the content is always emitted.
///
/// If you're looking for a way to only print something if the `Group` fits on a single line see [`self::if_group_fits_on_line`].
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("1,"),
///             soft_line_break_or_space(),
///             token("2,"),
///             soft_line_break_or_space(),
///             token("3"),
///             if_group_breaks(&token(","))
///         ]),
///         token("]"),
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
///
/// fn main() -> FormatResult<()> {
/// let context = SimpleFormatContext::new(SimpleFormatOptions {
///     line_width: LineWidth::try_from(20).unwrap(),
///     ..SimpleFormatOptions::default()
/// });
///
/// let elements = format!(context, [
///     group(&format_args![
///         token("["),
///         soft_block_indent(&format_args![
///             token("'A somewhat longer string to force a line break',"),
///             soft_line_break_or_space(),
///             token("2,"),
///             soft_line_break_or_space(),
///             token("3"),
///             if_group_breaks(&token(","))
///         ]),
///         token("]"),
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
/// See [`if_group_breaks`] if you're looking for a way to print content only for groups spanning multiple lines.
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("1,"),
///             soft_line_break_or_space(),
///             token("2,"),
///             soft_line_break_or_space(),
///             token("3"),
///             if_group_fits_on_line(&token(","))
///         ]),
///         token("]"),
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
///         token("["),
///         soft_block_indent(&format_args![
///             token("'A somewhat longer string to force a line break',"),
///             soft_line_break_or_space(),
///             token("2,"),
///             soft_line_break_or_space(),
///             token("3"),
///             if_group_fits_on_line(&token(","))
///         ]),
///         token("]"),
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
    ///                 token("["),
    ///                 soft_block_indent(&format_with(|f| {
    ///                     f.fill()
    ///                         .entry(&soft_line_break_or_space(), &token("1,"))
    ///                         .entry(&soft_line_break_or_space(), &token("234568789,"))
    ///                         .entry(&soft_line_break_or_space(), &token("3456789,"))
    ///                         .entry(&soft_line_break_or_space(), &format_args!(
    ///                             token("["),
    ///                             soft_block_indent(&token("4")),
    ///                             token("]"),
    ///                             if_group_breaks(&token(",")).with_group_id(Some(group_id))
    ///                         ))
    ///                     .finish()
    ///                 })),
    ///                 token("]")
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
    #[must_use]
    pub fn with_group_id(mut self, group_id: Option<GroupId>) -> Self {
        self.group_id = group_id;
        self
    }
}

impl<Context> Format<Context> for IfGroupBreaks<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartConditionalContent(
            Condition::new(self.mode).with_group_id(self.group_id),
        )));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndConditionalContent));

        Ok(())
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
/// This IR has the same semantics as using [`if_group_breaks`] and [`if_group_fits_on_line`] together.
///
/// ```
/// # use ruff_formatter::prelude::*;
/// # use ruff_formatter::write;
/// # let format = format_with(|f: &mut Formatter<SimpleFormatContext>| {
/// let id = f.group_id("head");
///
/// write!(f, [
///     group(&token("Head")).with_group_id(Some(id)),
///     if_group_breaks(&indent(&token("indented"))).with_group_id(Some(id)),
///     if_group_fits_on_line(&token("indented")).with_group_id(Some(id))
/// ])
///
/// # });
/// ```
///
/// If you want to indent some content if the enclosing group breaks, use [`indent`].
///
/// Use [`if_group_breaks`] or [`if_group_fits_on_line`] if the fitting and breaking content differs more than just the
/// indentation level.
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
///         group(&token("(aLongHeaderThatBreaksForSomeReason) =>")).with_group_id(Some(group_id)),
///         indent_if_group_breaks(&format_args![hard_line_break(), token("a => b")], group_id)
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
///         group(&token("(aLongHeaderThatBreaksForSomeReason) =>")).with_group_id(Some(group_id)),
///         indent_if_group_breaks(&format_args![hard_line_break(), token("a => b")], group_id)
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
        f.write_element(FormatElement::Tag(StartIndentIfGroupBreaks(self.group_id)));
        Arguments::from(&self.content).fmt(f)?;
        f.write_element(FormatElement::Tag(EndIndentIfGroupBreaks));

        Ok(())
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

/// Changes the definition of *fits* for `content`. It measures the width of all lines and allows
/// the content inside of the [`fits_expanded`] to exceed the configured line width. The content
/// coming before and after [`fits_expanded`] must fit into the configured line width.
///
/// The [`fits_expanded`] acts as a expands boundary similar to best fitting,
/// meaning that a [`hard_line_break`] will not cause the parent group to expand.
///
/// Useful in conjunction with a group with a condition.
///
/// ## Examples
/// The outer group with the binary expression remains *flat* regardless of the array expression that
/// spans multiple lines with items exceeding the configured line width.
///
/// ```
/// # use ruff_formatter::{format, format_args, LineWidth, SimpleFormatOptions, write};
/// # use ruff_formatter::prelude::*;
///
/// # fn main() -> FormatResult<()> {
/// let content = format_with(|f| {
///     let group_id = f.group_id("header");
///
///     write!(f, [
///         group(&format_args![
///             token("a"),
///             soft_line_break_or_space(),
///             token("+"),
///             space(),
///             fits_expanded(&group(&format_args![
///                 token("["),
///                 soft_block_indent(&format_args![
///                     token("a,"), space(), token("# comment"), expand_parent(), soft_line_break_or_space(),
///                     token("'A very long string that exceeds the configured line width of 80 characters but the enclosing binary expression still fits.'")
///                 ]),
///                 token("]")
///             ]))
///         ]),
///     ])
/// });
///
/// let formatted = format!(SimpleFormatContext::default(), [content])?;
///
/// assert_eq!(
///     "a + [\n\ta, # comment\n\t'A very long string that exceeds the configured line width of 80 characters but the enclosing binary expression still fits.'\n]",
///     formatted.print()?.as_code()
/// );
/// # Ok(())
/// # }
/// ```
pub fn fits_expanded<Content, Context>(content: &Content) -> FitsExpanded<Context>
where
    Content: Format<Context>,
{
    FitsExpanded {
        content: Argument::new(content),
        condition: None,
    }
}

#[derive(Clone)]
pub struct FitsExpanded<'a, Context> {
    content: Argument<'a, Context>,
    condition: Option<Condition>,
}

impl<Context> FitsExpanded<'_, Context> {
    /// Sets a `condition` to when the content should fit in expanded mode. The content uses the regular fits
    /// definition if the `condition` is not met.
    #[must_use]
    pub fn with_condition(mut self, condition: Option<Condition>) -> Self {
        self.condition = condition;
        self
    }
}

impl<Context> Format<Context> for FitsExpanded<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        f.write_element(FormatElement::Tag(StartFitsExpanded(
            tag::FitsExpanded::new().with_condition(self.condition),
        )));
        f.write_fmt(Arguments::from(&self.content))?;
        f.write_element(FormatElement::Tag(EndFitsExpanded));

        Ok(())
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
    #[inline]
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
///             token("("),
///             block_indent(&format_with(|f| {
///                 let separator = space();
///                 let mut join = f.join_with(&separator);
///
///                 for item in &self.items {
///                     join.entry(&format_with(|f| write!(f, [text(item)])));
///                 }
///                 join.finish()
///             })),
///             token(")")
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
/// fn generate_values() -> impl Iterator<Item=Token> {
///     vec![token("1"), token("2"), token("3"), token("4")].into_iter()
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
/// ```should_panic
/// use ruff_formatter::prelude::*;
/// use ruff_formatter::{SimpleFormatContext, format, write, Buffer};
/// use ruff_text_size::TextSize;
///
/// let mut count = 0;
///
/// let value = format_once(|f| {
///     write!(f, [text(&std::format!("Formatted {count}."))])
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
    #[inline]
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
/// See [`Formatter::join`]
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
        self.result = self.result.and_then(|()| {
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
        fmt.write_element(FormatElement::Tag(StartFill));

        Self {
            result: Ok(()),
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
        self.result = self.result.and_then(|()| {
            if self.empty {
                self.empty = false;
            } else {
                self.fmt.write_element(FormatElement::Tag(StartEntry));
                separator.fmt(self.fmt)?;
                self.fmt.write_element(FormatElement::Tag(EndEntry));
            }

            self.fmt.write_element(FormatElement::Tag(StartEntry));
            entry.fmt(self.fmt)?;
            self.fmt.write_element(FormatElement::Tag(EndEntry));
            Ok(())
        });

        self
    }

    /// Finishes the output and returns any error encountered
    pub fn finish(&mut self) -> FormatResult<()> {
        if self.result.is_ok() {
            self.fmt.write_element(FormatElement::Tag(EndFill));
        }
        self.result
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
    /// Creates a new best fitting IR with the given variants.
    ///
    /// Callers are required to ensure that the number of variants given
    /// is at least 2.
    ///
    /// You're looking for a way to create a `BestFitting` object, use the `best_fitting![least_expanded, most_expanded]` macro.
    ///
    /// # Panics
    ///
    /// When the slice contains less than two variants.
    pub fn from_arguments_unchecked(variants: Arguments<'a, Context>) -> Self {
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
    ///                     token("["),
    ///                         soft_block_indent(&format_args![
    ///                         token("1,"),
    ///                         soft_line_break_or_space(),
    ///                         token("2,"),
    ///                         soft_line_break_or_space(),
    ///                         token("3"),
    ///                     ]),
    ///                     token("]")
    ///                 ]),
    ///                 space(),
    ///                 token("+"),
    ///                 space(),
    ///                 token("aVeryLongIdentifier")
    ///             ),
    ///
    ///             // Breaks after `[` and prints each elements on a single line
    ///             // The group is necessary because the variant, by default is printed in flat mode and a
    ///             // hard line break indicates that the content doesn't fit.
    ///             format_args!(
    ///                 token("["),
    ///                 group(&block_indent(&format_args![token("1,"), hard_line_break(), token("2,"), hard_line_break(), token("3")])).should_expand(true),
    ///                 token("]"),
    ///                 space(),
    ///                 token("+"),
    ///                 space(),
    ///                 token("aVeryLongIdentifier")
    ///             ),
    ///
    ///             // Adds parentheses and indents the body, breaks after the operator
    ///             format_args!(
    ///                 token("("),
    ///                 block_indent(&format_args![
    ///                     token("["),
    ///                     block_indent(&format_args![
    ///                         token("1,"),
    ///                         hard_line_break(),
    ///                         token("2,"),
    ///                         hard_line_break(),
    ///                         token("3"),
    ///                     ]),
    ///                     token("]"),
    ///                     hard_line_break(),
    ///                     token("+"),
    ///                     space(),
    ///                     token("aVeryLongIdentifier")
    ///                 ]),
    ///                 token(")")
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
    #[must_use]
    pub fn with_mode(mut self, mode: BestFittingMode) -> Self {
        self.mode = mode;
        self
    }
}

impl<Context> Format<Context> for BestFitting<'_, Context> {
    fn fmt(&self, f: &mut Formatter<Context>) -> FormatResult<()> {
        let variants = self.variants.items();

        let mut buffer = VecBuffer::with_capacity(variants.len() * 8, f.state_mut());

        for variant in variants {
            buffer.write_element(FormatElement::Tag(StartBestFittingEntry));
            buffer.write_fmt(Arguments::from(variant))?;
            buffer.write_element(FormatElement::Tag(EndBestFittingEntry));
        }

        // OK because the constructor guarantees that there are always at
        // least two variants.
        let variants = BestFittingVariants::from_vec_unchecked(buffer.into_vec());
        let element = FormatElement::BestFitting {
            variants,
            mode: self.mode,
        };

        f.write_element(element);

        Ok(())
    }
}
