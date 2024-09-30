// This gives tons of false positives in this file because of
// "reStructuredText."
#![allow(clippy::doc_markdown)]

use std::cmp::Ordering;
use std::{borrow::Cow, collections::VecDeque};

use itertools::Itertools;

use ruff_formatter::printer::SourceMapGeneration;
use ruff_python_ast::{str::Quote, StringFlags};
use ruff_python_trivia::CommentRanges;
use {once_cell::sync::Lazy, regex::Regex};
use {
    ruff_formatter::{write, FormatOptions, IndentStyle, LineWidth, Printed},
    ruff_python_trivia::{is_python_whitespace, PythonWhitespace},
    ruff_source_file::Locator,
    ruff_text_size::{Ranged, TextLen, TextRange, TextSize},
};

use super::NormalizedString;
use crate::preview::is_docstring_code_block_in_docstring_indent_enabled;
use crate::string::StringQuotes;
use crate::{prelude::*, DocstringCodeLineWidth, FormatModuleError};

/// Format a docstring by trimming whitespace and adjusting the indentation.
///
/// Summary of changes we make:
/// * Normalize the string like all other strings
/// * Ignore docstring that have an escaped newline
/// * Trim all trailing whitespace, except for a chaperone space that avoids quotes or backslashes
///   in the last line.
/// * Trim leading whitespace on the first line, again except for a chaperone space
/// * If there is only content in the first line and after that only whitespace, collapse the
///   docstring into one line
/// * Adjust the indentation (see below)
///
/// # Docstring indentation
///
/// Unlike any other string, like black we change the indentation of docstring lines.
///
/// We want to preserve the indentation inside the docstring relative to the suite statement/block
/// indent that the docstring statement is in, but also want to apply the change of the outer
/// indentation in the docstring, e.g.
/// ```python
/// def sparkle_sky():
///   """Make a pretty sparkly sky.
///   *       * ✨        *.    .
///      *       *      ✨      .
///      .  *      . ✨    * .  .
///   """
/// ```
/// should become
/// ```python
/// def sparkle_sky():
///     """Make a pretty sparkly sky.
///     *       * ✨        *.    .
///        *       *      ✨      .
///        .  *      . ✨    * .  .
///     """
/// ```
/// We can't compute the full indentation here since we don't know what the block indent of
/// the doc comment will be yet and which we can only have added by formatting each line
/// separately with a hard line break. This means we need to strip shared indentation from
/// docstring while preserving the in-docstring bigger-than-suite-statement indentation. Example:
/// ```python
/// def f():
///  """first line
///  line a
///     line b
///  """
/// ```
/// The docstring indentation is 2, the block indents will change this to 4 (but we can't
/// determine this at this point). The indentation of line a is 2, so we trim `  line a`
/// to `line a`. For line b it's 5, so we trim it to `line b` and pad with 5-2=3 spaces to
/// `   line b`. The closing quotes, being on their own line, are stripped get only the
/// default indentation. Fully formatted:
/// ```python
/// def f():
///    """first line
///    line a
///       line b
///    """
/// ```
///
/// Tabs are counted by padding them to the next multiple of 8 according to
/// [`str.expandtabs`](https://docs.python.org/3/library/stdtypes.html#str.expandtabs).
///
/// Additionally, if any line in the docstring has less indentation than the docstring
/// (effectively a negative indentation wrt. to the current level), we pad all lines to the
/// level of the docstring with spaces.
/// ```python
/// def f():
///    """first line
/// line a
///    line b
///      line c
///    """
/// ```
/// Here line a is 3 columns negatively indented, so we pad all lines by an extra 3 spaces:
/// ```python
/// def f():
///    """first line
///    line a
///       line b
///         line c
///    """
/// ```
/// The indentation is rewritten to all-spaces when using [`IndentStyle::Space`].
/// The formatter preserves tab-indentations when using [`IndentStyle::Tab`], but doesn't convert
/// `indent-width * spaces` to tabs because doing so could break ASCII art and other docstrings
/// that use spaces for alignment.
pub(crate) fn format(normalized: &NormalizedString, f: &mut PyFormatter) -> FormatResult<()> {
    let docstring = &normalized.text();

    // Black doesn't change the indentation of docstrings that contain an escaped newline
    if contains_unescaped_newline(docstring) {
        return normalized.fmt(f);
    }

    // is_borrowed is unstable :/
    let already_normalized = matches!(docstring, Cow::Borrowed(_));

    // Use `split` instead of `lines` to preserve the closing quotes on their own line
    // if they have no indentation (in which case the last line is `\n` which
    // `lines` omit for the last element).
    let mut lines = docstring.split('\n').peekable();

    // Start the string
    let kind = normalized.flags();
    let quotes = StringQuotes::from(kind);
    write!(f, [kind.prefix(), quotes])?;
    // We track where in the source docstring we are (in source code byte offsets)
    let mut offset = normalized.start();

    // The first line directly after the opening quotes has different rules than the rest, mainly
    // that we remove all leading whitespace as there's no indentation
    let first = lines.next().unwrap_or_default();
    // Black trims whitespace using [`str.strip()`](https://docs.python.org/3/library/stdtypes.html#str.strip)
    // https://github.com/psf/black/blob/b4dca26c7d93f930bbd5a7b552807370b60d4298/src/black/strings.py#L77-L85
    // So we use the unicode whitespace definition through `trim_{start,end}` instead of the python
    // tokenizer whitespace definition in `trim_whitespace_{start,end}`.
    let trim_end = first.trim_end();
    let trim_both = trim_end.trim_start();

    // Edge case: The first line is `""" "content`, so we need to insert chaperone space that keep
    // inner quotes and closing quotes from getting to close to avoid `""""content`
    if trim_both.starts_with(quotes.quote_char.as_char()) {
        space().fmt(f)?;
    }

    if !trim_end.is_empty() {
        // For the first line of the docstring we strip the leading and trailing whitespace, e.g.
        // `"""   content   ` to `"""content`
        let leading_whitespace = trim_end.text_len() - trim_both.text_len();
        let trimmed_line_range =
            TextRange::at(offset, trim_end.text_len()).add_start(leading_whitespace);
        if already_normalized {
            source_text_slice(trimmed_line_range).fmt(f)?;
        } else {
            text(trim_both).fmt(f)?;
        }
    }
    offset += first.text_len();

    // Check if we have a single line (or empty) docstring
    if docstring[first.len()..].trim().is_empty() {
        // For `"""\n"""` or other whitespace between the quotes, black keeps a single whitespace,
        // but `""""""` doesn't get one inserted.
        if needs_chaperone_space(normalized, trim_end)
            || (trim_end.is_empty() && !docstring.is_empty())
        {
            space().fmt(f)?;
        }
        quotes.fmt(f)?;
        return Ok(());
    }

    hard_line_break().fmt(f)?;
    // We know that the normalized string has \n line endings
    offset += "\n".text_len();

    // If some line of the docstring is less indented than the function body, we pad all lines to
    // align it with the docstring statement. Conversely, if all lines are over-indented, we strip
    // the extra indentation. We call this stripped indentation since it's relative to the block
    // indent printer-made indentation.
    let stripped_indentation = lines
        .clone()
        // We don't want to count whitespace-only lines as miss-indented
        .filter(|line| !line.trim().is_empty())
        .map(Indentation::from_str)
        .min_by_key(|indentation| indentation.columns())
        .unwrap_or_default();

    DocstringLinePrinter {
        f,
        action_queue: VecDeque::new(),
        offset,
        stripped_indentation,
        already_normalized,
        quote_char: quotes.quote_char,
        code_example: CodeExample::default(),
    }
    .add_iter(lines)?;

    // Same special case in the last line as for the first line
    let trim_end = docstring
        .as_ref()
        .trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
    if needs_chaperone_space(normalized, trim_end) {
        space().fmt(f)?;
    }

    write!(f, [quotes])
}

fn contains_unescaped_newline(haystack: &str) -> bool {
    let mut rest = haystack;

    while let Some(index) = memchr::memchr(b'\\', rest.as_bytes()) {
        rest = rest[index + 1..].trim_whitespace_start();

        if rest.starts_with('\n') {
            return true;
        }
    }

    false
}

/// An abstraction for printing each line of a docstring.
struct DocstringLinePrinter<'ast, 'buf, 'fmt, 'src> {
    f: &'fmt mut PyFormatter<'ast, 'buf>,

    /// A queue of actions to perform.
    ///
    /// Whenever we process a line, it is possible for it to generate multiple
    /// actions to take. The most basic, and most common case, is for the line
    /// to just simply be printed as-is. But in some cases, a line is part of
    /// a code example that we'd like to reformat. In those cases, the actions
    /// can be more complicated.
    ///
    /// Actions are pushed on to the end of the queue and popped from the
    /// beginning.
    action_queue: VecDeque<CodeExampleAddAction<'src>>,

    /// The source offset of the beginning of the line that is currently being
    /// printed.
    offset: TextSize,

    /// Indentation alignment based on the least indented line in the
    /// docstring.
    stripped_indentation: Indentation,

    /// Whether the docstring is overall already considered normalized. When it
    /// is, the formatter can take a fast path.
    already_normalized: bool,

    /// The quote character used by the docstring being printed.
    quote_char: Quote,

    /// The current code example detected in the docstring.
    code_example: CodeExample<'src>,
}

impl<'ast, 'buf, 'fmt, 'src> DocstringLinePrinter<'ast, 'buf, 'fmt, 'src> {
    /// Print all of the lines in the given iterator to this
    /// printer's formatter.
    ///
    /// Note that callers may treat the first line specially, such that the
    /// iterator given contains all lines except for the first.
    fn add_iter(
        &mut self,
        mut lines: std::iter::Peekable<std::str::Split<'src, char>>,
    ) -> FormatResult<()> {
        while let Some(line) = lines.next() {
            let line = InputDocstringLine {
                line,
                offset: self.offset,
                next: lines.peek().copied(),
            };
            // We know that the normalized string has \n line endings.
            self.offset += line.line.text_len() + "\n".text_len();
            self.add_one(line)?;
        }
        self.code_example.finish(&mut self.action_queue);
        self.run_action_queue()
    }

    /// Adds the given line to this printer.
    ///
    /// Depending on what's in the line, this may or may not print the line
    /// immediately to the underlying buffer. If the line starts or is part
    /// of an existing code snippet, then the lines will get buffered until
    /// the code snippet is complete.
    fn add_one(&mut self, line: InputDocstringLine<'src>) -> FormatResult<()> {
        // Just pass through the line as-is without looking for a code snippet
        // when docstring code formatting is disabled. And also when we are
        // formatting a code snippet so as to avoid arbitrarily nested code
        // snippet formatting. We avoid this because it's likely quite tricky
        // to get right 100% of the time, although perhaps not impossible. It's
        // not clear that it's worth the effort to support.
        if !self.f.options().docstring_code().is_enabled() || self.f.context().docstring().is_some()
        {
            return self.print_one(&line.as_output());
        }
        self.code_example.add(line, &mut self.action_queue);
        self.run_action_queue()
    }

    /// Process any actions in this printer's queue until the queue is empty.
    fn run_action_queue(&mut self) -> FormatResult<()> {
        while let Some(action) = self.action_queue.pop_front() {
            match action {
                CodeExampleAddAction::Print { original } => {
                    self.print_one(&original.as_output())?;
                }
                CodeExampleAddAction::Kept => {}
                CodeExampleAddAction::Reset { code } => {
                    for codeline in code {
                        self.print_one(&codeline.original.as_output())?;
                    }
                }
                CodeExampleAddAction::Format { mut kind } => {
                    let Some(formatted_lines) = self.format(&mut kind)? else {
                        // Since we've failed to emit these lines, we need to
                        // put them back in the queue but have them jump to the
                        // front of the queue to get processed before any other
                        // action.
                        self.action_queue.push_front(CodeExampleAddAction::Reset {
                            code: kind.into_code(),
                        });
                        continue;
                    };

                    self.already_normalized = false;
                    match kind {
                        CodeExampleKind::Doctest(CodeExampleDoctest { ps1_indent, .. }) => {
                            let mut lines = formatted_lines.into_iter();
                            let Some(first) = lines.next() else { continue };
                            self.print_one(
                                &first.map(|line| std::format!("{ps1_indent}>>> {line}")),
                            )?;
                            for docline in lines {
                                self.print_one(
                                    &docline.map(|line| std::format!("{ps1_indent}... {line}")),
                                )?;
                            }
                        }
                        CodeExampleKind::Rst(litblock) => {
                            let Some(min_indent) = litblock.min_indent else {
                                continue;
                            };
                            // This looks suspicious, but it's consistent with the whitespace
                            // normalization that will occur anyway.
                            let indent = " ".repeat(min_indent.columns());
                            for docline in formatted_lines {
                                self.print_one(
                                    &docline.map(|line| std::format!("{indent}{line}")),
                                )?;
                            }
                        }
                        CodeExampleKind::Markdown(fenced) => {
                            // This looks suspicious, but it's consistent with the whitespace
                            // normalization that will occur anyway.
                            let indent = " ".repeat(fenced.opening_fence_indent.columns());
                            for docline in formatted_lines {
                                self.print_one(
                                    &docline.map(|line| std::format!("{indent}{line}")),
                                )?;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Prints the single line given.
    ///
    /// This mostly just handles indentation and ensuring line breaks are
    /// inserted as appropriate before passing it on to the formatter to
    /// print to the buffer.
    fn print_one(&mut self, line: &OutputDocstringLine<'_>) -> FormatResult<()> {
        let trim_end = line.line.trim_end();
        if trim_end.is_empty() {
            return if line.is_last {
                // If the doc string ends with `    """`, the last line is
                // `    `, but we don't want to insert an empty line (but close
                // the docstring).
                Ok(())
            } else {
                empty_line().fmt(self.f)
            };
        }

        let indent_offset = match self.f.options().indent_style() {
            // Normalize all indent to spaces.
            IndentStyle::Space => {
                let tab_or_non_ascii_space = trim_end
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .any(|c| c != ' ');

                if tab_or_non_ascii_space {
                    None
                } else {
                    // It's guaranteed that the `indent` is all spaces because `tab_or_non_ascii_space` is
                    // `false` (indent contains neither tabs nor non-space whitespace).
                    let stripped_indentation_len = self.stripped_indentation.text_len();

                    // Take the string with the trailing whitespace removed, then also
                    // skip the leading whitespace.
                    Some(stripped_indentation_len)
                }
            }
            IndentStyle::Tab => {
                let line_indent = Indentation::from_str(trim_end);

                let non_ascii_whitespace = trim_end
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .any(|c| !matches!(c, ' ' | '\t'));

                let trimmed = line_indent.trim_start(self.stripped_indentation);

                // Preserve tabs that are used for indentation, but only if the indent isn't
                // * a mix of tabs and spaces
                // * the `stripped_indentation` is a prefix of the line's indent
                // * the trimmed indent isn't spaces followed by tabs because that would result in a
                //   mixed tab, spaces, tab indentation, resulting in instabilities.
                let preserve_indent = !non_ascii_whitespace
                    && trimmed.is_some_and(|trimmed| !trimmed.is_spaces_tabs());
                preserve_indent.then_some(self.stripped_indentation.text_len())
            }
        };

        if let Some(indent_offset) = indent_offset {
            // Take the string with the trailing whitespace removed, then also
            // skip the leading whitespace.
            if self.already_normalized {
                let trimmed_line_range =
                    TextRange::at(line.offset, trim_end.text_len()).add_start(indent_offset);
                source_text_slice(trimmed_line_range).fmt(self.f)?;
            } else {
                text(&trim_end[indent_offset.to_usize()..]).fmt(self.f)?;
            }
        } else {
            // We strip the indentation that is shared with the docstring
            // statement, unless a line was indented less than the docstring
            // statement, in which case we strip only this much indentation to
            // implicitly pad all lines by the difference, or all lines were
            // overindented, in which case we strip the additional whitespace
            // (see example in [`format_docstring`] doc comment). We then
            // prepend the in-docstring indentation to the string.
            let indent_len =
                Indentation::from_str(trim_end).columns() - self.stripped_indentation.columns();
            let in_docstring_indent = " ".repeat(indent_len) + trim_end.trim_start();
            text(&in_docstring_indent).fmt(self.f)?;
        };

        // We handled the case that the closing quotes are on their own line
        // above (the last line is empty except for whitespace). If they are on
        // the same line as content, we don't insert a line break.
        if !line.is_last {
            hard_line_break().fmt(self.f)?;
        }

        Ok(())
    }

    /// Given a code example, format them and return
    /// the formatted code as a sequence of owned docstring lines.
    ///
    /// This may mutate the code example in place if extracting the lines of
    /// code requires adjusting which part of each line is used for the actual
    /// code bit.
    ///
    /// This routine generally only returns an error when the recursive call
    /// to the formatter itself returns a `FormatError`. In all other cases
    /// (for example, if the code snippet is invalid Python or even if the
    /// resulting reformatted code snippet is invalid Python), then `Ok(None)`
    /// is returned. In this case, callers should assume that a reformatted
    /// code snippet is unavailable and bail out of trying to format it.
    ///
    /// Currently, when the above cases happen and `Ok(None)` is returned, the
    /// routine is silent about it. So from the user's perspective, this will
    /// fail silently. Ideally, this would at least emit a warning message,
    /// but at time of writing, it wasn't clear to me how to best do that.
    fn format(
        &mut self,
        kind: &mut CodeExampleKind<'_>,
    ) -> FormatResult<Option<Vec<OutputDocstringLine<'static>>>> {
        use ruff_python_parser::AsMode;

        let line_width = match self.f.options().docstring_code_line_width() {
            DocstringCodeLineWidth::Fixed(width) => width,
            DocstringCodeLineWidth::Dynamic => {
                let global_line_width = self.f.options().line_width().value();
                let indent_width = self.f.options().indent_width();
                let indent_level = self.f.context().indent_level();
                let mut current_indent = indent_level
                    .to_ascii_spaces(indent_width)
                    .saturating_add(kind.extra_indent_ascii_spaces());

                if is_docstring_code_block_in_docstring_indent_enabled(self.f.context()) {
                    // Add the in-docstring indentation
                    current_indent = current_indent.saturating_add(
                        u16::try_from(
                            kind.indent()
                                .columns()
                                .saturating_sub(self.stripped_indentation.columns()),
                        )
                        .unwrap_or(u16::MAX),
                    );
                }

                let width = std::cmp::max(1, global_line_width.saturating_sub(current_indent));
                LineWidth::try_from(width).expect("width should be capped at a minimum of 1")
            }
        };

        let code = kind.code();
        let (Some(unformatted_first), Some(unformatted_last)) = (code.first(), code.last()) else {
            return Ok(None);
        };
        let codeblob = code
            .iter()
            .map(|line| line.code)
            .collect::<Vec<&str>>()
            .join("\n");
        let options = self
            .f
            .options()
            .clone()
            .with_line_width(line_width)
            // It's perhaps a little odd to be hard-coding the indent
            // style here, but I believe it is necessary as a result
            // of the whitespace normalization otherwise done in
            // docstrings. Namely, tabs are rewritten with ASCII
            // spaces. If code examples in docstrings are formatted
            // with tabs and those tabs end up getting rewritten, this
            // winds up screwing with the indentation in ways that
            // results in formatting no longer being idempotent. Since
            // tabs will get erased anyway, we just clobber them here
            // instead of later, and as a result, get more consistent
            // results.
            .with_indent_style(IndentStyle::Space)
            .with_source_map_generation(SourceMapGeneration::Disabled);
        let printed = match docstring_format_source(options, self.quote_char, &codeblob) {
            Ok(printed) => printed,
            Err(FormatModuleError::FormatError(err)) => return Err(err),
            Err(FormatModuleError::ParseError(_) | FormatModuleError::PrintError(_)) => {
                return Ok(None);
            }
        };
        // This is a little hokey, but we want to determine whether the
        // reformatted code snippet will lead to an overall invalid docstring.
        // So attempt to parse it as Python code, but ensure it is wrapped
        // within a docstring using the same quotes as the docstring we're in
        // right now.
        //
        // This is an unfortunate stop-gap to attempt to prevent us from
        // writing invalid Python due to some oddity of the code snippet within
        // a docstring. As we fix corner cases over time, we can perhaps
        // remove this check. See the `doctest_invalid_skipped` tests in
        // `docstring_code_examples.py` for when this check is relevant.
        let wrapped = match self.quote_char {
            Quote::Single => std::format!("'''{}'''", printed.as_code()),
            Quote::Double => {
                std::format!(r#""""{}""""#, printed.as_code())
            }
        };
        let result = ruff_python_parser::parse(&wrapped, self.f.options().source_type().as_mode());
        // If the resulting code is not valid, then reset and pass through
        // the docstring lines as-is.
        if result.is_err() {
            return Ok(None);
        }
        let mut lines = printed
            .as_code()
            .lines()
            .map(|line| OutputDocstringLine {
                line: Cow::Owned(line.to_string()),
                offset: unformatted_first.original.offset,
                is_last: false,
            })
            .collect::<Vec<_>>();
        if let Some(reformatted_last) = lines.last_mut() {
            reformatted_last.is_last = unformatted_last.original.is_last();
        }
        Ok(Some(lines))
    }
}

/// Represents a single line in a docstring.
///
/// This type is only used to represent the original lines in a docstring.
/// Specifically, the line contained in this type has no changes from the input
/// source.
#[derive(Clone, Copy, Debug)]
struct InputDocstringLine<'src> {
    /// The actual text of the line, not including the line terminator.
    ///
    /// In practice, this line is borrowed when it corresponds to an original
    /// unformatted line in a docstring, and owned when it corresponds to a
    /// reformatted line (e.g., from a code snippet) in a docstring.
    line: &'src str,

    /// The offset into the source document which this line corresponds to.
    offset: TextSize,

    /// For any input line that isn't the last line, this contains a reference
    /// to the line immediately following this one.
    ///
    /// This is `None` if and only if this is the last line in the docstring.
    next: Option<&'src str>,
}

impl<'src> InputDocstringLine<'src> {
    /// Borrow this input docstring line as an output docstring line.
    fn as_output(&self) -> OutputDocstringLine<'src> {
        OutputDocstringLine {
            line: Cow::Borrowed(self.line),
            offset: self.offset,
            is_last: self.is_last(),
        }
    }

    /// Whether this is the last line in the docstring or not.
    fn is_last(&self) -> bool {
        self.next.is_none()
    }
}

/// Represents a single reformatted code line in a docstring.
///
/// An input source line may be cheaply converted to an output source line.
/// This is the common case: an input source line is printed pretty much as it
/// is, with perhaps some whitespace normalization applied. The less common
/// case is that the output docstring line owns its `line` because it was
/// produced by reformatting a code snippet.
#[derive(Clone, Debug)]
struct OutputDocstringLine<'src> {
    /// The output line.
    ///
    /// This is an owned variant in precisely the cases where it corresponds to
    /// a line from a reformatted code snippet. In other cases, it is borrowed
    /// from the input docstring line as-is.
    line: Cow<'src, str>,

    /// The offset into the source document which this line corresponds to.
    /// Currently, this is an estimate.
    offset: TextSize,

    /// Whether this is the last line in a docstring or not. This is determined
    /// by whether the last line in the code snippet was also the last line in
    /// the docstring. If it was, then it follows that the last line in the
    /// reformatted code snippet is also the last line in the docstring.
    is_last: bool,
}

impl<'src> OutputDocstringLine<'src> {
    /// Return this reformatted line, but with the given function applied to
    /// the text of the line.
    fn map(self, mut map: impl FnMut(&str) -> String) -> OutputDocstringLine<'static> {
        OutputDocstringLine {
            line: Cow::Owned(map(&self.line)),
            ..self
        }
    }
}

/// A single code example extracted from a docstring.
///
/// This represents an intermediate state from when the code example was first
/// found all the way up until the point at which the code example has finished
/// and is reformatted.
///
/// Its default state is "empty." That is, that no code example is currently
/// being collected.
#[derive(Debug, Default)]
struct CodeExample<'src> {
    /// The kind of code example being collected, or `None` if no code example
    /// has been observed.
    ///
    /// The kind is split out into a separate type so that we can pass it
    /// around and have a guarantee that a code example actually exists.
    kind: Option<CodeExampleKind<'src>>,
}

impl<'src> CodeExample<'src> {
    /// Attempt to add an original line from a docstring to this code example.
    ///
    /// Based on the line and the internal state of whether a code example is
    /// currently being collected or not, this will push an "action" to the
    /// given queue for the caller to perform. The typical case is a "print"
    /// action, which instructs the caller to just print the line as though it
    /// were not part of a code snippet.
    fn add(
        &mut self,
        original: InputDocstringLine<'src>,
        queue: &mut VecDeque<CodeExampleAddAction<'src>>,
    ) {
        match self.kind.take() {
            // There's no existing code example being built, so we look for
            // the start of one or otherwise tell the caller we couldn't find
            // anything.
            None => {
                self.add_start(original, queue);
            }
            Some(CodeExampleKind::Doctest(doctest)) => {
                let Some(doctest) = doctest.add_code_line(original, queue) else {
                    self.add_start(original, queue);
                    return;
                };
                self.kind = Some(CodeExampleKind::Doctest(doctest));
            }
            Some(CodeExampleKind::Rst(litblock)) => {
                let Some(litblock) = litblock.add_code_line(original, queue) else {
                    self.add_start(original, queue);
                    return;
                };
                self.kind = Some(CodeExampleKind::Rst(litblock));
            }
            Some(CodeExampleKind::Markdown(fenced)) => {
                let Some(fenced) = fenced.add_code_line(original, queue) else {
                    // For Markdown, the last line in a block should be printed
                    // as-is. Especially since the last line in many Markdown
                    // fenced code blocks is identical to the start of a code
                    // block. So if we try to start a new code block with
                    // the last line, we risk opening another Markdown block
                    // inappropriately.
                    return;
                };
                self.kind = Some(CodeExampleKind::Markdown(fenced));
            }
        }
    }

    /// Finish the code example by generating any final actions if applicable.
    ///
    /// This typically adds an action when the end of a code example coincides
    /// with the end of the docstring.
    fn finish(&mut self, queue: &mut VecDeque<CodeExampleAddAction<'src>>) {
        let Some(kind) = self.kind.take() else { return };
        queue.push_back(CodeExampleAddAction::Format { kind });
    }

    /// Looks for the start of a code example. If one was found, then the given
    /// line is kept and added as part of the code example. Otherwise, the line
    /// is pushed onto the queue unchanged to be printed as-is.
    ///
    /// # Panics
    ///
    /// This panics when the existing code-example is any non-None value. That
    /// is, this routine assumes that there is no ongoing code example being
    /// collected and looks for the beginning of another code example.
    fn add_start(
        &mut self,
        original: InputDocstringLine<'src>,
        queue: &mut VecDeque<CodeExampleAddAction<'src>>,
    ) {
        assert!(self.kind.is_none(), "expected no existing code example");
        if let Some(doctest) = CodeExampleDoctest::new(original) {
            self.kind = Some(CodeExampleKind::Doctest(doctest));
            queue.push_back(CodeExampleAddAction::Kept);
        } else if let Some(litblock) = CodeExampleRst::new(original) {
            self.kind = Some(CodeExampleKind::Rst(litblock));
            queue.push_back(CodeExampleAddAction::Print { original });
        } else if let Some(fenced) = CodeExampleMarkdown::new(original) {
            self.kind = Some(CodeExampleKind::Markdown(fenced));
            queue.push_back(CodeExampleAddAction::Print { original });
        } else {
            queue.push_back(CodeExampleAddAction::Print { original });
        }
    }
}

/// The kind of code example observed in a docstring.
#[derive(Debug)]
enum CodeExampleKind<'src> {
    /// Code found in Python "doctests."
    ///
    /// Documentation describing doctests and how they're recognized can be
    /// found as part of the Python standard library:
    /// https://docs.python.org/3/library/doctest.html.
    ///
    /// (You'll likely need to read the [regex matching] used internally by the
    /// doctest module to determine more precisely how it works.)
    ///
    /// [regex matching]: https://github.com/python/cpython/blob/0ff6368519ed7542ad8b443de01108690102420a/Lib/doctest.py#L611-L622
    Doctest(CodeExampleDoctest<'src>),
    /// Code found from a reStructuredText "[literal block]" or "[code block
    /// directive]".
    ///
    /// [literal block]: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#literal-blocks
    /// [code block directive]: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html#directive-code-block
    Rst(CodeExampleRst<'src>),
    /// Code found from a Markdown "[fenced code block]".
    ///
    /// [fenced code block]: https://spec.commonmark.org/0.30/#fenced-code-blocks
    Markdown(CodeExampleMarkdown<'src>),
}

impl<'src> CodeExampleKind<'src> {
    /// Return the lines of code collected so far for this example.
    ///
    /// This is borrowed mutably because it may need to mutate the code lines
    /// based on the state accrued so far.
    fn code(&mut self) -> &[CodeExampleLine<'src>] {
        match *self {
            CodeExampleKind::Doctest(ref doctest) => &doctest.lines,
            CodeExampleKind::Rst(ref mut litblock) => litblock.indented_code(),
            CodeExampleKind::Markdown(ref fenced) => &fenced.lines,
        }
    }

    /// Consume this code example and return only the lines that have been
    /// accrued so far.
    ///
    /// This is useful when the code example being collected has been
    /// determined to be invalid, and one wants to "give up" and print the
    /// original lines through unchanged without attempting formatting.
    fn into_code(self) -> Vec<CodeExampleLine<'src>> {
        match self {
            CodeExampleKind::Doctest(doctest) => doctest.lines,
            CodeExampleKind::Rst(litblock) => litblock.lines,
            CodeExampleKind::Markdown(fenced) => fenced.lines,
        }
    }

    /// This returns any extra indent that will be added after formatting this
    /// code example.
    ///
    /// The extra indent is expressed in units of ASCII space characters.
    fn extra_indent_ascii_spaces(&self) -> u16 {
        match *self {
            CodeExampleKind::Doctest(_) => 4,
            _ => 0,
        }
    }

    /// The indent of the entire code block relative to the start of the line.
    ///
    /// For example:
    /// ```python
    /// def test():
    ///     """Docstring
    ///     Example:
    ///        >>> 1 + 1
    /// ```
    ///
    /// The `>>> ` block has an indent of 8 columns: The shared indent with the docstring and the 4 spaces
    /// inside the docstring.
    fn indent(&self) -> Indentation {
        match self {
            CodeExampleKind::Doctest(doctest) => Indentation::from_str(doctest.ps1_indent),
            CodeExampleKind::Rst(rst) => rst.min_indent.unwrap_or(rst.opening_indent),
            CodeExampleKind::Markdown(markdown) => markdown.opening_fence_indent,
        }
    }
}

/// State corresponding to a single doctest code example found in a docstring.
#[derive(Debug)]
struct CodeExampleDoctest<'src> {
    /// The lines that have been seen so far that make up the doctest.
    lines: Vec<CodeExampleLine<'src>>,

    /// The indent observed in the first doctest line.
    ///
    /// More precisely, this corresponds to the whitespace observed before
    /// the starting `>>> ` (the "PS1 prompt").
    ps1_indent: &'src str,
}

impl<'src> CodeExampleDoctest<'src> {
    /// Looks for a valid doctest PS1 prompt in the line given.
    ///
    /// If one was found, then state for a new doctest code example is
    /// returned, along with the code example line.
    fn new(original: InputDocstringLine<'src>) -> Option<CodeExampleDoctest<'src>> {
        let trim_start = original.line.trim_start();
        // Prompts must be followed by an ASCII space character[1].
        //
        // [1]: https://github.com/python/cpython/blob/0ff6368519ed7542ad8b443de01108690102420a/Lib/doctest.py#L809-L812
        let code = trim_start.strip_prefix(">>> ")?;
        let indent_len = original
            .line
            .len()
            .checked_sub(trim_start.len())
            .expect("suffix is <= original");
        let lines = vec![CodeExampleLine { original, code }];
        let ps1_indent = &original.line[..indent_len];
        let doctest = CodeExampleDoctest { lines, ps1_indent };
        Some(doctest)
    }

    /// Looks for a valid doctest PS2 prompt in the line given. If one is
    /// found, it is added to this code example and ownership of the example is
    /// returned to the caller. In this case, callers should continue trying to
    /// add PS2 prompt lines.
    ///
    /// But if one isn't found, then the given line is not part of the code
    /// example and ownership of this example is not returned.
    ///
    /// In either case, relevant actions will be added to the given queue to
    /// process.
    fn add_code_line(
        mut self,
        original: InputDocstringLine<'src>,
        queue: &mut VecDeque<CodeExampleAddAction<'src>>,
    ) -> Option<CodeExampleDoctest<'src>> {
        let Some((ps2_indent, ps2_after)) = original.line.split_once("...") else {
            queue.push_back(self.into_format_action());
            return None;
        };
        // PS2 prompts must have the same indentation as their
        // corresponding PS1 prompt.[1] While the 'doctest' Python
        // module will error in this case, we just treat this line as a
        // non-doctest line.
        //
        // [1]: https://github.com/python/cpython/blob/0ff6368519ed7542ad8b443de01108690102420a/Lib/doctest.py#L733
        if self.ps1_indent != ps2_indent {
            queue.push_back(self.into_format_action());
            return None;
        }
        // PS2 prompts must be followed by an ASCII space character unless
        // it's an otherwise empty line[1].
        //
        // [1]: https://github.com/python/cpython/blob/0ff6368519ed7542ad8b443de01108690102420a/Lib/doctest.py#L809-L812
        let code = match ps2_after.strip_prefix(' ') {
            None if ps2_after.is_empty() => "",
            None => {
                queue.push_back(self.into_format_action());
                return None;
            }
            Some(code) => code,
        };
        self.lines.push(CodeExampleLine { original, code });
        queue.push_back(CodeExampleAddAction::Kept);
        Some(self)
    }

    /// Consume this doctest and turn it into a formatting action.
    fn into_format_action(self) -> CodeExampleAddAction<'src> {
        CodeExampleAddAction::Format {
            kind: CodeExampleKind::Doctest(self),
        }
    }
}

/// State corresponding to a single reStructuredText literal block or
/// code-block directive.
///
/// While a literal block and code-block directive are technically two
/// different reStructuredText constructs, we use one type to represent
/// both because they are exceptionally similar. Basically, they are
/// the same with two main differences:
///
/// 1. Literal blocks are began with a line that ends with `::`. Code block
///    directives are began with a line like `.. code-block:: python`.
/// 2. Code block directives permit a list of options as a "field list"
///    immediately after the opening line. Literal blocks have no options.
///
/// Otherwise, everything else, including the indentation structure, is the
/// same.
#[derive(Debug)]
struct CodeExampleRst<'src> {
    /// The lines that have been seen so far that make up the block.
    lines: Vec<CodeExampleLine<'src>>,

    /// The indent of the line "opening" this block in columns.
    ///
    /// It can either be the indent of a line ending with `::` (for a literal
    /// block) or the indent of a line starting with `.. ` (a directive).
    ///
    /// The content body of a block needs to be indented more than the line
    /// opening the block, so we use this indentation to look for indentation
    /// that is "more than" it.
    opening_indent: Indentation,

    /// The minimum indent of the block in columns.
    ///
    /// This is `None` until the first such line is seen. If no such line is
    /// found, then we consider it an invalid block and bail out of trying to
    /// find a code snippet. Otherwise, we update this indentation as we see
    /// lines in the block with less indentation. (Usually, the minimum is the
    /// indentation of the first block, but this is not required.)
    ///
    /// By construction, all lines part of the block must have at least this
    /// indentation. Additionally, it is guaranteed that the indentation length
    /// of the opening indent is strictly less than the indentation of the
    /// minimum indent. Namely, the block ends once we find a line that has
    /// been unindented to at most the indent of the opening line.
    ///
    /// When the code snippet has been extracted, it is re-built before being
    /// reformatted. The minimum indent is stripped from each line when it is
    /// re-built.
    min_indent: Option<Indentation>,

    /// Whether this is a directive block or not. When not a directive, this is
    /// a literal block. The main difference between them is that they start
    /// differently. A literal block is started merely by trailing a line with
    /// `::`. A directive block is started with `.. code-block:: python`.
    ///
    /// The other difference is that directive blocks can have options
    /// (represented as a reStructuredText "field list") after the beginning of
    /// the directive and before the body content of the directive.
    is_directive: bool,
}

impl<'src> CodeExampleRst<'src> {
    /// Looks for the start of a reStructuredText [literal block] or [code
    /// block directive].
    ///
    /// If the start of a block is found, then this returns a correctly
    /// initialized reStructuredText block. Callers should print the line as
    /// given as it is not retained as part of the block.
    ///
    /// [literal block]: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#literal-blocks
    /// [code block directive]: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html#directive-code-block
    fn new(original: InputDocstringLine<'src>) -> Option<CodeExampleRst> {
        let (opening_indent, rest) = indent_with_suffix(original.line);
        if rest.starts_with(".. ") {
            if let Some(litblock) = CodeExampleRst::new_code_block(original) {
                return Some(litblock);
            }
            // In theory, we could still have something that looks like a literal block,
            // but if the line starts with `.. `, then it seems like it probably shouldn't
            // be a literal block. For example:
            //
            //     .. code-block::
            //
            //         cool_stuff( 1 )
            //
            // The above is not valid because the `language` argument is missing from
            // the `code-block` directive. Because of how we handle it here, the above
            // is not treated as a code snippet.
            return None;
        }
        // At this point, we know we didn't find a code block, so the only
        // thing we can hope for is a literal block which must end with a `::`.
        if !rest.trim_end().ends_with("::") {
            return None;
        }
        Some(CodeExampleRst {
            lines: vec![],
            opening_indent: Indentation::from_str(opening_indent),
            min_indent: None,
            is_directive: false,
        })
    }

    /// Attempts to create a new reStructuredText code example from a
    /// `code-block` or `sourcecode` directive. If one couldn't be found, then
    /// `None` is returned.
    fn new_code_block(original: InputDocstringLine<'src>) -> Option<CodeExampleRst> {
        // This regex attempts to parse the start of a reStructuredText code
        // block [directive]. From the reStructuredText spec:
        //
        // > Directives are indicated by an explicit markup start (".. ")
        // > followed by the directive type, two colons, and whitespace
        // > (together called the "directive marker"). Directive types
        // > are case-insensitive single words (alphanumerics plus
        // > isolated internal hyphens, underscores, plus signs, colons,
        // > and periods; no whitespace).
        //
        // The language names matched here (e.g., `python` or `py`) are taken
        // from the [Pygments lexer names], which is referenced from the docs
        // for the [code-block] directive.
        //
        // [directives]: https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#directives
        // [Pygments lexer names]: https://pygments.org/docs/lexers/
        // [code-block]: https://www.sphinx-doc.org/en/master/usage/restructuredtext/directives.html#directive-code-block
        static DIRECTIVE_START: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"(?m)^\s*\.\. \s*(?i:code-block|sourcecode)::\s*(?i:python|py|python3|py3)$",
            )
            .unwrap()
        });
        if !DIRECTIVE_START.is_match(original.line) {
            return None;
        }
        Some(CodeExampleRst {
            lines: vec![],
            opening_indent: Indentation::from_str(original.line),
            min_indent: None,
            is_directive: true,
        })
    }

    /// Returns the code collected in this example as a sequence of lines.
    ///
    /// The lines returned have the minimum indentation stripped from their
    /// prefix in-place. Based on the definition of minimum indentation, this
    /// implies there is at least one line in the slice returned with no
    /// whitespace prefix.
    fn indented_code(&mut self) -> &[CodeExampleLine<'src>] {
        let Some(min_indent) = self.min_indent else {
            return &[];
        };
        for line in &mut self.lines {
            line.code = if line.original.line.trim().is_empty() {
                ""
            } else {
                min_indent.trim_start_str(line.original.line)
            };
        }
        &self.lines
    }

    /// Attempts to add the given line from a docstring to the reStructuredText
    /// code snippet being collected.
    ///
    /// This takes ownership of `self`, and if ownership is returned to the
    /// caller, that means the caller should continue trying to add lines to
    /// this code snippet. Otherwise, if ownership is not returned, then this
    /// implies at least one action was added to the give queue to either reset
    /// the code block or format. That is, the code snippet was either found to
    /// be invalid or it was completed and should be reformatted.
    ///
    /// Note that actions may be added even if ownership is returned. For
    /// example, empty lines immediately preceding the actual code snippet will
    /// be returned back as an action to print them verbatim, but the caller
    /// should still continue to try to add lines to this code snippet.
    fn add_code_line(
        mut self,
        original: InputDocstringLine<'src>,
        queue: &mut VecDeque<CodeExampleAddAction<'src>>,
    ) -> Option<CodeExampleRst<'src>> {
        // If we haven't started populating the minimum indent yet, then
        // we haven't found the first code line and may need to find and
        // pass through leading empty lines.
        let Some(min_indent) = self.min_indent else {
            return self.add_first_line(original, queue);
        };
        let (indent, rest) = indent_with_suffix(original.line);
        if rest.is_empty() {
            // This is the standard way we close a block: when we see
            // an empty line followed by an unindented non-empty line.
            if let Some(next) = original.next {
                let (next_indent, next_rest) = indent_with_suffix(next);
                if !next_rest.is_empty()
                    && Indentation::from_str(next_indent) <= self.opening_indent
                {
                    self.push_format_action(queue);
                    return None;
                }
            } else {
                self.push_format_action(queue);
                return None;
            }
            self.push(original);
            queue.push_back(CodeExampleAddAction::Kept);
            return Some(self);
        }
        let indent_len = Indentation::from_str(indent);
        if indent_len <= self.opening_indent {
            // If we find an unindented non-empty line at the same (or less)
            // indentation of the opening line at this point, then we know it
            // must be wrong because we didn't see it immediately following an
            // empty line.
            queue.push_back(self.into_reset_action());
            return None;
        } else if indent_len < min_indent {
            // While the minimum indent is usually the indentation of the first
            // line in a code snippet, it is not guaranteed to be the case.
            // And indeed, reST is happy to let blocks have a first line whose
            // indentation is greater than a subsequent line in the block. The
            // only real restriction is that every line in the block must be
            // indented at least past the indentation of the `::` line.
            self.min_indent = Some(indent_len);
        }
        self.push(original);
        queue.push_back(CodeExampleAddAction::Kept);
        Some(self)
    }

    /// Looks for the first line in a literal or code block.
    ///
    /// If a first line is found, then this returns true. Otherwise, an empty
    /// line has been found and the caller should pass it through to the
    /// docstring unchanged. (Empty lines are allowed to precede a
    /// block. And there must be at least one of them.)
    ///
    /// If the given line is invalid for a reStructuredText block (i.e., no
    /// empty lines seen between the opening line), then an error variant is
    /// returned. In this case, callers should bail out of parsing this code
    /// example.
    ///
    /// When this returns `true`, it is guaranteed that `self.min_indent` is
    /// set to a non-None value.
    ///
    /// # Panics
    ///
    /// Callers must only call this when the first indentation has not yet been
    /// found. If it has, then this panics.
    fn add_first_line(
        mut self,
        original: InputDocstringLine<'src>,
        queue: &mut VecDeque<CodeExampleAddAction<'src>>,
    ) -> Option<CodeExampleRst<'src>> {
        assert!(self.min_indent.is_none());

        // While the rst spec isn't completely clear on this point, through
        // experimentation, I found that multiple empty lines before the first
        // non-empty line are ignored.
        let (indent, rest) = indent_with_suffix(original.line);
        if rest.is_empty() {
            queue.push_back(CodeExampleAddAction::Print { original });
            return Some(self);
        }
        // Ignore parameters in field lists. These can only occur in
        // directives, not literal blocks.
        if self.is_directive && is_rst_option(rest) {
            queue.push_back(CodeExampleAddAction::Print { original });
            return Some(self);
        }
        let min_indent = Indentation::from_str(indent);
        // At this point, we found a non-empty line. The only thing we require
        // is that its indentation is strictly greater than the indentation of
        // the line containing the `::`. Otherwise, we treat this as an invalid
        // block and bail.
        if min_indent <= self.opening_indent {
            queue.push_back(self.into_reset_action());
            return None;
        }
        self.min_indent = Some(min_indent);
        self.push(original);
        queue.push_back(CodeExampleAddAction::Kept);
        Some(self)
    }

    /// Pushes the given line as part of this code example.
    fn push(&mut self, original: InputDocstringLine<'src>) {
        // N.B. We record the code portion as identical to the original line.
        // When we go to reformat the code lines, we change them by removing
        // the `min_indent`. This design is necessary because the true value of
        // `min_indent` isn't known until the entire block has been parsed.
        let code = original.line;
        self.lines.push(CodeExampleLine { original, code });
    }

    /// Consume this block and add actions to the give queue for formatting.
    ///
    /// This may trim lines from the end of the block and add them to the queue
    /// for printing as-is. For example, this happens when there are trailing
    /// empty lines, as we would like to preserve those since they aren't
    /// generally treated as part of the code block.
    fn push_format_action(mut self, queue: &mut VecDeque<CodeExampleAddAction<'src>>) {
        let has_non_whitespace = |line: &CodeExampleLine| {
            line.original
                .line
                .chars()
                .any(|ch| !is_python_whitespace(ch))
        };
        let first_trailing_empty_line = self
            .lines
            .iter()
            .rposition(has_non_whitespace)
            .map_or(0, |i| i + 1);
        let trailing_lines = self.lines.split_off(first_trailing_empty_line);
        queue.push_back(CodeExampleAddAction::Format {
            kind: CodeExampleKind::Rst(self),
        });
        queue.extend(
            trailing_lines
                .into_iter()
                .map(|line| CodeExampleAddAction::Print {
                    original: line.original,
                }),
        );
    }

    /// Consume this block and turn it into a reset action.
    ///
    /// This occurs when we started collecting a code example from something
    /// that looked like a block, but later determined that it wasn't a valid
    /// block.
    fn into_reset_action(self) -> CodeExampleAddAction<'src> {
        CodeExampleAddAction::Reset { code: self.lines }
    }
}

/// Represents a code example extracted from a Markdown [fenced code block].
///
/// [fenced code block]: https://spec.commonmark.org/0.30/#fenced-code-blocks
#[derive(Debug)]
struct CodeExampleMarkdown<'src> {
    /// The lines that have been seen so far that make up the block.
    lines: Vec<CodeExampleLine<'src>>,

    /// The indent of the line "opening" fence of this block in columns.
    ///
    /// This indentation is trimmed from the indentation of every line in the
    /// body of the code block,
    opening_fence_indent: Indentation,

    /// The kind of fence, backticks or tildes, used for this block. We need to
    /// keep track of which kind was used to open the block in order to look
    /// for a correct close of the block.
    fence_kind: MarkdownFenceKind,

    /// The size of the fence, in codepoints, in the opening line. A correct
    /// close of the fence must use *at least* this many characters. In other
    /// words, this is the number of backticks or tildes that opened the fenced
    /// code block.
    fence_len: usize,
}

impl<'src> CodeExampleMarkdown<'src> {
    /// Looks for the start of a Markdown [fenced code block].
    ///
    /// If the start of a block is found, then this returns a correctly
    /// initialized Markdown code block. Callers should print the line as given
    /// as it is not retained as part of the block.
    ///
    /// [fenced code block]: https://spec.commonmark.org/0.30/#fenced-code-blocks
    fn new(original: InputDocstringLine<'src>) -> Option<CodeExampleMarkdown<'src>> {
        static FENCE_START: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"(?xm)
                ^
                (?:
                    # In the backtick case, info strings (following the fence)
                    # cannot contain backticks themselves, since it would
                    # introduce ambiguity with parsing inline code. In other
                    # words, if we didn't specifically exclude matching `
                    # in the info string for backtick fences, then we might
                    # erroneously consider something to be a code fence block
                    # that is actually inline code.
                    #
                    # NOTE: The `ticklang` and `tildlang` capture groups are
                    # currently unused, but there was some discussion about not
                    # assuming unlabeled blocks were Python. At the time of
                    # writing, we do assume unlabeled blocks are Python, but
                    # one could inspect the `ticklang` and `tildlang` capture
                    # groups to determine whether the block is labeled or not.
                    (?<ticks>```+)(?:\s*(?<ticklang>(?i:python|py|python3|py3))[^`]*)?
                    |
                    (?<tilds>~~~+)(?:\s*(?<tildlang>(?i:python|py|python3|py3))\p{any}*)?
                )
                $
                ",
            )
            .unwrap()
        });

        let (opening_fence_indent, rest) = indent_with_suffix(original.line);
        // Quit quickly in the vast majority of cases.
        if !rest.starts_with("```") && !rest.starts_with("~~~") {
            return None;
        }

        let caps = FENCE_START.captures(rest)?;
        let (fence_kind, fence_len) = if let Some(ticks) = caps.name("ticks") {
            (MarkdownFenceKind::Backtick, ticks.as_str().chars().count())
        } else {
            let tildes = caps
                .name("tilds")
                .expect("no ticks means it must be tildes");
            (MarkdownFenceKind::Tilde, tildes.as_str().chars().count())
        };
        Some(CodeExampleMarkdown {
            lines: vec![],
            opening_fence_indent: Indentation::from_str(opening_fence_indent),
            fence_kind,
            fence_len,
        })
    }

    /// Attempts to add the given line from a docstring to the Markdown code
    /// snippet being collected.
    ///
    /// In this case, ownership is only not returned when the end of the block
    /// was found, or if the block was determined to be invalid. A formatting
    /// action is then pushed onto the queue.
    fn add_code_line(
        mut self,
        original: InputDocstringLine<'src>,
        queue: &mut VecDeque<CodeExampleAddAction<'src>>,
    ) -> Option<CodeExampleMarkdown<'src>> {
        if self.is_end(original) {
            queue.push_back(self.into_format_action());
            queue.push_back(CodeExampleAddAction::Print { original });
            return None;
        }
        // When a line in a Markdown fenced closed block is indented *less*
        // than the opening indent, we treat the entire block as invalid.
        //
        // I believe that code blocks of this form are actually valid Markdown
        // in some cases, but the interplay between it and our docstring
        // whitespace normalization leads to undesirable outcomes. For example,
        // if the line here is unindented out beyond the initial indent of the
        // docstring itself, then this causes the entire docstring to have
        // its indent normalized. And, at the time of writing, a subsequent
        // formatting run undoes this indentation, thus violating idempotency.
        if !original.line.trim_whitespace().is_empty()
            && Indentation::from_str(original.line) < self.opening_fence_indent
        {
            queue.push_back(self.into_reset_action());
            queue.push_back(CodeExampleAddAction::Print { original });
            return None;
        }
        self.push(original);
        queue.push_back(CodeExampleAddAction::Kept);
        Some(self)
    }

    /// Returns true when given line ends this fenced code block.
    fn is_end(&self, original: InputDocstringLine<'src>) -> bool {
        let (_, rest) = indent_with_suffix(original.line);
        // We can bail early if we don't have at least three backticks or
        // tildes.
        if !rest.starts_with("```") && !rest.starts_with("~~~") {
            return false;
        }
        // We do need to check that we have the right number of
        // backticks/tildes...
        let fence_len = rest
            .chars()
            .take_while(|&ch| ch == self.fence_kind.to_char())
            .count();
        // A closing fence only needs *at least* the number of ticks/tildes
        // that are in the opening fence.
        if fence_len < self.fence_len {
            return false;
        }
        // And, also, there can only be trailing whitespace. Nothing else.
        assert!(
            self.fence_kind.to_char().is_ascii(),
            "fence char should be ASCII",
        );
        if !rest[fence_len..].chars().all(is_python_whitespace) {
            return false;
        }
        true
    }

    /// Pushes the given line as part of this code example.
    fn push(&mut self, original: InputDocstringLine<'src>) {
        // Unlike reStructuredText blocks, for Markdown fenced code blocks, the
        // indentation that we want to strip from each line is known when the
        // block is opened. So we can strip it as we collect lines.
        let code = self.opening_fence_indent.trim_start_str(original.line);
        self.lines.push(CodeExampleLine { original, code });
    }

    /// Consume this block and turn it into a reset action.
    ///
    /// This occurs when we started collecting a code example from something
    /// that looked like a block, but later determined that it wasn't a valid
    /// block.
    fn into_format_action(self) -> CodeExampleAddAction<'src> {
        // Note that unlike in reStructuredText blocks, if a Markdown fenced
        // code block is unclosed, then *all* remaining lines should be treated
        // as part of the block[1]:
        //
        // > If the end of the containing block (or document) is reached and no
        // > closing code fence has been found, the code block contains all of the
        // > lines after the opening code fence until the end of the containing
        // > block (or document).
        //
        // This means that we don't need to try and trim trailing empty lines.
        // Those will get fed into the code formatter and ultimately stripped,
        // which is what you'd expect if those lines are treated as part of the
        // block.
        //
        // [1]: https://spec.commonmark.org/0.30/#fenced-code-blocks
        CodeExampleAddAction::Format {
            kind: CodeExampleKind::Markdown(self),
        }
    }

    /// Consume this block and turn it into a reset action.
    ///
    /// This occurs when we started collecting a code example from something
    /// that looked like a code fence, but later determined that it wasn't a
    /// valid.
    fn into_reset_action(self) -> CodeExampleAddAction<'src> {
        CodeExampleAddAction::Reset { code: self.lines }
    }
}

/// The kind of fence used in a Markdown code block.
///
/// This indicates that the fence is either surrounded by fences made from
/// backticks, or fences made from tildes.
#[derive(Clone, Copy, Debug)]
enum MarkdownFenceKind {
    Backtick,
    Tilde,
}

impl MarkdownFenceKind {
    /// Convert the fence kind to the actual character used to build the fence.
    fn to_char(self) -> char {
        match self {
            MarkdownFenceKind::Backtick => '`',
            MarkdownFenceKind::Tilde => '~',
        }
    }
}

/// A single line in a code example found in a docstring.
///
/// A code example line exists prior to formatting, and is thus in full
/// correspondence with the original lines from the docstring. Indeed, a
/// code example line includes both the original line *and* the actual code
/// extracted from the line. For example, if a line in a docstring is `>>>
/// foo(x)`, then the original line is `>>> foo(x)` and the code portion is
/// `foo(x)`.
///
/// The original line is kept for things like offset information, but also
/// because it may still be needed if it turns out that the code snippet is
/// not valid or otherwise could not be formatted. In which case, the original
/// lines are printed as-is.
#[derive(Debug)]
struct CodeExampleLine<'src> {
    /// The normalized (but original) line from the doc string. This might, for
    /// example, contain a `>>> ` or `... ` prefix if this code example is a
    /// doctest.
    original: InputDocstringLine<'src>,

    /// The code extracted from the line.
    code: &'src str,
}

/// An action that a caller should perform after attempting to add a line from
/// a docstring to a code example.
///
/// Callers are expected to add every line from a docstring to a code example,
/// and the state of the code example (and the line itself) will determine
/// how the caller should react.
#[derive(Debug)]
enum CodeExampleAddAction<'src> {
    /// The line added was ignored by `CodeExample` and the caller should print
    /// it to the formatter as-is.
    ///
    /// This is the common case. That is, most lines in most docstrings are not
    /// part of a code example.
    Print { original: InputDocstringLine<'src> },
    /// The line added was kept by `CodeExample` as part of a new or existing
    /// code example.
    ///
    /// When this occurs, callers should not try to format the line and instead
    /// move on to the next line.
    Kept,
    /// The line added indicated that the code example is finished and should
    /// be formatted and printed. The line added is not treated as part of
    /// the code example.
    Format {
        /// The kind of code example that was found.
        kind: CodeExampleKind<'src>,
    },
    /// This occurs when adding a line to an existing code example
    /// results in that code example becoming invalid. In this case,
    /// we don't want to treat it as a code example, but instead write
    /// back the lines to the docstring unchanged.
    Reset {
        /// The lines of code that we collected but should be printed back to
        /// the docstring as-is and not formatted.
        code: Vec<CodeExampleLine<'src>>,
    },
}

/// Formats the given source code using the given options.
///
/// The given quote style should correspond to the style used by the docstring
/// containing the code snippet being formatted. The formatter will use this
/// information to invert the quote style of any such strings contained within
/// the code snippet in order to avoid writing invalid Python code.
///
/// This is similar to the top-level formatting entrypoint, except this
/// explicitly sets the context to indicate that formatting is taking place
/// inside of a docstring.
fn docstring_format_source(
    options: crate::PyFormatOptions,
    docstring_quote_style: Quote,
    source: &str,
) -> Result<Printed, FormatModuleError> {
    use ruff_python_parser::AsMode;

    let source_type = options.source_type();
    let parsed = ruff_python_parser::parse(source, source_type.as_mode())?;
    let comment_ranges = CommentRanges::from(parsed.tokens());
    let source_code = ruff_formatter::SourceCode::new(source);
    let comments = crate::Comments::from_ast(parsed.syntax(), source_code, &comment_ranges);
    let locator = Locator::new(source);

    let ctx = PyFormatContext::new(options, locator.contents(), comments, parsed.tokens())
        .in_docstring(docstring_quote_style);
    let formatted = crate::format!(ctx, [parsed.syntax().format()])?;
    formatted
        .context()
        .comments()
        .assert_all_formatted(source_code);
    Ok(formatted.print()?)
}

/// If the last line of the docstring is `content" """` or `content\ """`, we need a chaperone space
/// that avoids `content""""` and `content\"""`. This does only applies to un-escaped backslashes,
/// so `content\\ """` doesn't need a space while `content\\\ """` does.
fn needs_chaperone_space(normalized: &NormalizedString, trim_end: &str) -> bool {
    trim_end.ends_with(normalized.flags().quote_style().as_char())
        || trim_end.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

#[derive(Copy, Clone, Debug)]
enum Indentation {
    /// Space only indentation or an empty indentation.
    ///
    /// The value is the number of spaces.
    Spaces(usize),

    /// Tabs only indentation.
    Tabs(usize),

    /// Indentation that uses tabs followed by spaces.
    /// Also known as smart tabs where tabs are used for indents, and spaces for alignment.
    TabSpaces { tabs: usize, spaces: usize },

    /// Indentation that uses spaces followed by tabs.
    SpacesTabs { spaces: usize, tabs: usize },

    /// Mixed indentation of tabs and spaces.
    Mixed {
        /// The visual width of the indentation in columns.
        width: usize,

        /// The length of the indentation in bytes
        len: TextSize,
    },
}

impl Indentation {
    const TAB_INDENT_WIDTH: usize = 8;

    fn from_str(s: &str) -> Self {
        let mut iter = s.chars().peekable();

        let spaces = iter.peeking_take_while(|c| *c == ' ').count();
        let tabs = iter.peeking_take_while(|c| *c == '\t').count();

        if tabs == 0 {
            // No indent, or spaces only indent
            return Self::Spaces(spaces);
        }

        let align_spaces = iter.peeking_take_while(|c| *c == ' ').count();

        if spaces == 0 {
            if align_spaces == 0 {
                return Self::Tabs(tabs);
            }

            // At this point it's either a smart tab (tabs followed by spaces) or a wild mix of tabs and spaces.
            if iter.peek().copied() != Some('\t') {
                return Self::TabSpaces {
                    tabs,
                    spaces: align_spaces,
                };
            }
        } else if align_spaces == 0 {
            return Self::SpacesTabs { spaces, tabs };
        }

        // Sequence of spaces.. tabs, spaces, tabs...
        let mut width = spaces + tabs * Self::TAB_INDENT_WIDTH + align_spaces;
        // SAFETY: Safe because Ruff doesn't support files larger than 4GB.
        let mut len = TextSize::try_from(spaces + tabs + align_spaces).unwrap();

        for char in iter {
            if char == '\t' {
                // Pad to the next multiple of tab_width
                width += Self::TAB_INDENT_WIDTH - (width.rem_euclid(Self::TAB_INDENT_WIDTH));
                len += '\t'.text_len();
            } else if char.is_whitespace() {
                width += char.len_utf8();
                len += char.text_len();
            } else {
                break;
            }
        }

        // Mixed tabs and spaces
        Self::Mixed { width, len }
    }

    /// Returns the indentation's visual width in columns/spaces.
    ///
    /// For docstring indentation, black counts spaces as 1 and tabs by increasing the indentation up
    /// to the next multiple of 8. This is effectively a port of
    /// [`str.expandtabs`](https://docs.python.org/3/library/stdtypes.html#str.expandtabs),
    /// which black [calls with the default tab width of 8](https://github.com/psf/black/blob/c36e468794f9256d5e922c399240d49782ba04f1/src/black/strings.py#L61).
    const fn columns(self) -> usize {
        match self {
            Self::Spaces(count) => count,
            Self::Tabs(count) => count * Self::TAB_INDENT_WIDTH,
            Self::TabSpaces { tabs, spaces } => tabs * Self::TAB_INDENT_WIDTH + spaces,
            Self::SpacesTabs { spaces, tabs } => {
                let mut indent = spaces;
                indent += Self::TAB_INDENT_WIDTH - indent.rem_euclid(Self::TAB_INDENT_WIDTH);
                indent + (tabs - 1) * Self::TAB_INDENT_WIDTH
            }
            Self::Mixed { width, .. } => width,
        }
    }

    /// Returns the length of the indentation in bytes.
    ///
    /// # Panics
    /// If the indentation is longer than 4GB.
    fn text_len(self) -> TextSize {
        let len = match self {
            Self::Spaces(count) => count,
            Self::Tabs(count) => count,
            Self::TabSpaces { tabs, spaces } => tabs + spaces,
            Self::SpacesTabs { spaces, tabs } => spaces + tabs,
            Self::Mixed { len, .. } => return len,
        };

        TextSize::try_from(len).unwrap()
    }

    /// Trims the indent of `rhs` by `self`.
    ///
    /// Returns `None` if `self` is not a prefix of `rhs` or either `self` or `rhs` use mixed indentation.
    fn trim_start(self, rhs: Self) -> Option<Self> {
        let (left_tabs, left_spaces) = match self {
            Self::Spaces(spaces) => (0usize, spaces),
            Self::Tabs(tabs) => (tabs, 0usize),
            Self::TabSpaces { tabs, spaces } => (tabs, spaces),
            // Handle spaces here because it is the only indent where the spaces come before the tabs.
            Self::SpacesTabs {
                spaces: left_spaces,
                tabs: left_tabs,
            } => {
                return match rhs {
                    Self::Spaces(right_spaces) => {
                        left_spaces.checked_sub(right_spaces).map(|spaces| {
                            if spaces == 0 {
                                Self::Tabs(left_tabs)
                            } else {
                                Self::SpacesTabs {
                                    tabs: left_tabs,
                                    spaces,
                                }
                            }
                        })
                    }
                    Self::SpacesTabs {
                        spaces: right_spaces,
                        tabs: right_tabs,
                    } => left_spaces.checked_sub(right_spaces).and_then(|spaces| {
                        let tabs = left_tabs.checked_sub(right_tabs)?;

                        Some(if spaces == 0 {
                            if tabs == 0 {
                                Self::Spaces(0)
                            } else {
                                Self::Tabs(tabs)
                            }
                        } else {
                            Self::SpacesTabs { spaces, tabs }
                        })
                    }),

                    _ => None,
                }
            }
            Self::Mixed { .. } => return None,
        };

        let (right_tabs, right_spaces) = match rhs {
            Self::Spaces(spaces) => (0usize, spaces),
            Self::Tabs(tabs) => (tabs, 0usize),
            Self::TabSpaces { tabs, spaces } => (tabs, spaces),
            Self::SpacesTabs { .. } | Self::Mixed { .. } => return None,
        };

        let tabs = left_tabs.checked_sub(right_tabs)?;
        let spaces = left_spaces.checked_sub(right_spaces)?;

        Some(if tabs == 0 {
            Self::Spaces(spaces)
        } else if spaces == 0 {
            Self::Tabs(tabs)
        } else {
            Self::TabSpaces { tabs, spaces }
        })
    }

    /// Trims at most `indent_len` indentation from the beginning of `line`.
    ///
    /// This is useful when one needs to trim some minimum
    /// level of indentation from a code snippet collected from a docstring before
    /// attempting to reformat it.
    fn trim_start_str(self, line: &str) -> &str {
        let mut seen_indent_len = 0;
        let mut trimmed = line;
        let indent_len = self.columns();

        for char in line.chars() {
            if seen_indent_len >= indent_len {
                return trimmed;
            }
            if char == '\t' {
                // Pad to the next multiple of tab_width
                seen_indent_len +=
                    Self::TAB_INDENT_WIDTH - (seen_indent_len.rem_euclid(Self::TAB_INDENT_WIDTH));
                trimmed = &trimmed[1..];
            } else if char.is_whitespace() {
                seen_indent_len += char.len_utf8();
                trimmed = &trimmed[char.len_utf8()..];
            } else {
                break;
            }
        }
        trimmed
    }

    const fn is_spaces_tabs(self) -> bool {
        matches!(self, Self::SpacesTabs { .. })
    }
}

impl PartialOrd for Indentation {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.columns().cmp(&other.columns()))
    }
}

impl PartialEq for Indentation {
    fn eq(&self, other: &Self) -> bool {
        self.columns() == other.columns()
    }
}

impl Default for Indentation {
    fn default() -> Self {
        Self::Spaces(0)
    }
}

/// Returns the indentation of the given line and everything following it.
fn indent_with_suffix(line: &str) -> (&str, &str) {
    let suffix = line.trim_whitespace_start();
    let indent_len = line
        .len()
        .checked_sub(suffix.len())
        .expect("suffix <= line");
    let indent = &line[..indent_len];
    (indent, suffix)
}

/// Returns true if this line looks like a reStructuredText option in a
/// field list.
///
/// That is, a line that looks like `:name: optional-value`.
fn is_rst_option(line: &str) -> bool {
    let line = line.trim_start();
    if !line.starts_with(':') {
        return false;
    }
    line.chars()
        .take_while(|&ch| !is_python_whitespace(ch))
        .any(|ch| ch == ':')
}

#[cfg(test)]
mod tests {
    use crate::string::docstring::Indentation;

    #[test]
    fn indentation_like_black() {
        assert_eq!(Indentation::from_str("\t \t  \t").columns(), 24);
        assert_eq!(Indentation::from_str("\t        \t").columns(), 24);
        assert_eq!(Indentation::from_str("\t\t\t").columns(), 24);
        assert_eq!(Indentation::from_str("    ").columns(), 4);
    }
}
