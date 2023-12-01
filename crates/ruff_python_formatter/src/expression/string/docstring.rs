use std::borrow::Cow;

use ruff_python_trivia::PythonWhitespace;
use {
    ruff_formatter::{write, Printed},
    ruff_source_file::Locator,
    ruff_text_size::{Ranged, TextLen, TextRange, TextSize},
};

use crate::{prelude::*, FormatModuleError, QuoteStyle};

use super::NormalizedString;

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
/// [`str.expandtabs`](https://docs.python.org/3/library/stdtypes.html#str.expandtabs). When
/// we see indentation that contains a tab or any other none ascii-space whitespace we rewrite the
/// string.
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
pub(super) fn format(normalized: &NormalizedString, f: &mut PyFormatter) -> FormatResult<()> {
    let docstring = &normalized.text;

    // Black doesn't change the indentation of docstrings that contain an escaped newline
    if contains_unescaped_newline(docstring) {
        return normalized.fmt(f);
    }

    // is_borrowed is unstable :/
    let already_normalized = matches!(docstring, Cow::Borrowed(_));

    let mut lines = docstring.lines().peekable();

    // Start the string
    write!(
        f,
        [
            normalized.prefix,
            normalized.quotes,
            source_position(normalized.start()),
        ]
    )?;
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
    if trim_both.starts_with(normalized.quotes.style.as_char()) {
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
            text(trim_both, Some(trimmed_line_range.start())).fmt(f)?;
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
        normalized.quotes.fmt(f)?;
        return Ok(());
    }

    hard_line_break().fmt(f)?;
    // We know that the normalized string has \n line endings
    offset += "\n".text_len();

    // If some line of the docstring is less indented than the function body, we pad all lines to
    // align it with the docstring statement. Conversely, if all lines are over-indented, we strip
    // the extra indentation. We call this stripped indentation since it's relative to the block
    // indent printer-made indentation.
    let stripped_indentation_length = lines
        .clone()
        // We don't want to count whitespace-only lines as miss-indented
        .filter(|line| !line.trim().is_empty())
        .map(indentation_length)
        .min()
        .unwrap_or_default();

    DocstringLinePrinter {
        f,
        offset,
        stripped_indentation_length,
        already_normalized,
        quote_style: normalized.quotes.style,
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

    write!(f, [source_position(normalized.end()), normalized.quotes])
}

fn contains_unescaped_newline(haystack: &str) -> bool {
    let mut rest = haystack;

    while let Some(index) = memchr::memchr(b'\\', rest.as_bytes()) {
        rest = &rest[index + 1..].trim_whitespace_start();

        if rest.starts_with('\n') {
            return true;
        }
    }

    false
}

/// An abstraction for printing each line of a docstring.
struct DocstringLinePrinter<'ast, 'buf, 'fmt, 'src> {
    f: &'fmt mut PyFormatter<'ast, 'buf>,
    /// The source offset of the beginning of the line that is currently being
    /// printed.
    offset: TextSize,
    /// Indentation alignment based on the least indented line in the
    /// docstring.
    stripped_indentation_length: TextSize,
    /// Whether the docstring is overall already considered normalized. When it
    /// is, the formatter can take a fast path.
    already_normalized: bool,
    /// The quote style used by the docstring being printed.
    quote_style: QuoteStyle,
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
        mut lines: std::iter::Peekable<std::str::Lines<'src>>,
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
        Ok(())
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
        match self.code_example.add(line) {
            CodeExampleAddAction::Print { original } => self.print_one(&original.as_output())?,
            CodeExampleAddAction::Kept => {}
            CodeExampleAddAction::Reset { code, original } => {
                for codeline in code {
                    self.print_one(&codeline.original.as_output())?;
                }
                self.print_one(&original.as_output())?;
            }
            CodeExampleAddAction::Format { mut kind, original } => {
                let Some(formatted_lines) = self.format(kind.code())? else {
                    // If formatting failed in a way that should not be
                    // allowed, we back out what we're doing and print the
                    // original lines we found as-is as if we did nothing.
                    for codeline in kind.code() {
                        self.print_one(&codeline.original.as_output())?;
                    }
                    if let Some(original) = original {
                        self.print_one(&original.as_output())?;
                    }
                    return Ok(());
                };

                self.already_normalized = false;
                match kind {
                    CodeExampleKind::Doctest(CodeExampleDoctest { ps1_indent, .. }) => {
                        let mut lines = formatted_lines.into_iter();
                        if let Some(first) = lines.next() {
                            self.print_one(
                                &first.map(|line| std::format!("{ps1_indent}>>> {line}")),
                            )?;
                            for docline in lines {
                                self.print_one(
                                    &docline.map(|line| std::format!("{ps1_indent}... {line}")),
                                )?;
                            }
                        }
                    }
                }
                if let Some(original) = original {
                    self.print_one(&original.as_output())?;
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

        let tab_or_non_ascii_space = trim_end
            .chars()
            .take_while(|c| c.is_whitespace())
            .any(|c| c != ' ');

        if tab_or_non_ascii_space {
            // We strip the indentation that is shared with the docstring
            // statement, unless a line was indented less than the docstring
            // statement, in which case we strip only this much indentation to
            // implicitly pad all lines by the difference, or all lines were
            // overindented, in which case we strip the additional whitespace
            // (see example in [`format_docstring`] doc comment). We then
            // prepend the in-docstring indentation to the string.
            let indent_len = indentation_length(trim_end) - self.stripped_indentation_length;
            let in_docstring_indent = " ".repeat(usize::from(indent_len)) + trim_end.trim_start();
            text(&in_docstring_indent, Some(line.offset)).fmt(self.f)?;
        } else {
            // Take the string with the trailing whitespace removed, then also
            // skip the leading whitespace.
            let trimmed_line_range = TextRange::at(line.offset, trim_end.text_len())
                .add_start(self.stripped_indentation_length);
            if self.already_normalized {
                source_text_slice(trimmed_line_range).fmt(self.f)?;
            } else {
                // All indents are ascii spaces, so the slicing is correct.
                text(
                    &trim_end[usize::from(self.stripped_indentation_length)..],
                    Some(trimmed_line_range.start()),
                )
                .fmt(self.f)?;
            }
        }

        // We handled the case that the closing quotes are on their own line
        // above (the last line is empty except for whitespace). If they are on
        // the same line as content, we don't insert a line break.
        if !line.is_last {
            hard_line_break().fmt(self.f)?;
        }

        Ok(())
    }

    /// Given a sequence of lines from a code snippet, format them and return
    /// the formatted code as a sequence of owned docstring lines.
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
    ///
    /// # Panics
    ///
    /// This panics when the given slice is empty.
    fn format(
        &mut self,
        code: &[CodeExampleLine<'_>],
    ) -> FormatResult<Option<Vec<OutputDocstringLine<'static>>>> {
        use ruff_python_parser::AsMode;

        let offset = code
            .get(0)
            .expect("code blob must be non-empty")
            .original
            .offset;
        let last_line_is_last = code
            .last()
            .expect("code blob must be non-empty")
            .original
            .is_last();
        let codeblob = code
            .iter()
            .map(|line| line.code)
            .collect::<Vec<&str>>()
            .join("\n");
        let printed = match docstring_format_source(self.f.options(), self.quote_style, &codeblob) {
            Ok(printed) => printed,
            Err(FormatModuleError::FormatError(err)) => return Err(err),
            Err(
                FormatModuleError::LexError(_)
                | FormatModuleError::ParseError(_)
                | FormatModuleError::PrintError(_),
            ) => {
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
        let wrapped = match self.quote_style {
            QuoteStyle::Single => std::format!("'''{}'''", printed.as_code()),
            QuoteStyle::Double => std::format!(r#""""{}""""#, printed.as_code()),
        };
        let result = ruff_python_parser::parse(
            &wrapped,
            self.f.options().source_type().as_mode(),
            "<filename>",
        );
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
                offset,
                is_last: false,
            })
            .collect::<Vec<_>>();
        if let Some(last) = lines.last_mut() {
            last.is_last = last_line_is_last;
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
    /// currently being collected or not, this will return an "action" for
    /// the caller to perform. The typical case is a "print" action, which
    /// instructs the caller to just print the line as though it were not part
    /// of a code snippet.
    fn add(&mut self, original: InputDocstringLine<'src>) -> CodeExampleAddAction<'src> {
        match self.kind.take() {
            // There's no existing code example being built, so we look for
            // the start of one or otherwise tell the caller we couldn't find
            // anything.
            None => match self.add_start(original) {
                None => CodeExampleAddAction::Kept,
                Some(original) => CodeExampleAddAction::Print { original },
            },
            Some(CodeExampleKind::Doctest(mut doctest)) => {
                if doctest.add_code_line(original) {
                    // Stay with the doctest kind while we accumulate all
                    // PS2 prompts.
                    self.kind = Some(CodeExampleKind::Doctest(doctest));
                    return CodeExampleAddAction::Kept;
                }
                let original = self.add_start(original);
                CodeExampleAddAction::Format {
                    kind: CodeExampleKind::Doctest(doctest),
                    original,
                }
            }
        }
    }

    /// Looks for the start of a code example. If one was found, then the given
    /// line is kept and added as part of the code example. Otherwise, the line
    /// is returned unchanged and no code example was found.
    ///
    /// # Panics
    ///
    /// This panics when the existing code-example is any non-None value. That
    /// is, this routine assumes that there is no ongoing code example being
    /// collected and looks for the beginning of another code example.
    fn add_start(
        &mut self,
        original: InputDocstringLine<'src>,
    ) -> Option<InputDocstringLine<'src>> {
        assert!(self.kind.is_none(), "expected no existing code example");
        if let Some(doctest) = CodeExampleDoctest::new(original) {
            self.kind = Some(CodeExampleKind::Doctest(doctest));
            return None;
        }
        Some(original)
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
}

impl<'src> CodeExampleKind<'src> {
    /// Return the lines of code collected so far for this example.
    ///
    /// This is borrowed mutably because it may need to mutate the code lines
    /// based on the state accrued so far.
    fn code(&mut self) -> &[CodeExampleLine<'src>] {
        match *self {
            CodeExampleKind::Doctest(ref doctest) => &doctest.lines,
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

    /// Looks for a valid doctest PS2 prompt in the line given.
    ///
    /// If one is found, then the code portion of the line following the PS2 prompt
    /// is returned.
    ///
    /// Callers must provide a string containing the original indentation of the
    /// PS1 prompt that started the doctest containing the potential PS2 prompt
    /// in the line given. If the line contains a PS2 prompt, its indentation must
    /// match the indentation used for the corresponding PS1 prompt (otherwise
    /// `None` will be returned).
    fn add_code_line(&mut self, original: InputDocstringLine<'src>) -> bool {
        let Some((ps2_indent, ps2_after)) = original.line.split_once("...") else {
            return false;
        };
        // PS2 prompts must have the same indentation as their
        // corresponding PS1 prompt.[1] While the 'doctest' Python
        // module will error in this case, we just treat this line as a
        // non-doctest line.
        //
        // [1]: https://github.com/python/cpython/blob/0ff6368519ed7542ad8b443de01108690102420a/Lib/doctest.py#L733
        if self.ps1_indent != ps2_indent {
            return false;
        }
        // PS2 prompts must be followed by an ASCII space character unless
        // it's an otherwise empty line[1].
        //
        // [1]: https://github.com/python/cpython/blob/0ff6368519ed7542ad8b443de01108690102420a/Lib/doctest.py#L809-L812
        let code = match ps2_after.strip_prefix(' ') {
            None if ps2_after.is_empty() => "",
            None => return false,
            Some(code) => code,
        };
        self.lines.push(CodeExampleLine { original, code });
        true
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
    /// the code example. If the line added indicated the start of another
    /// code example, then is won't be returned to the caller here. Otherwise,
    /// callers should pass it through to the formatter as-is.
    Format {
        /// The kind of code example that was found.
        ///
        /// This is guaranteed to have a non-empty code snippet.
        kind: CodeExampleKind<'src>,
        /// When set, the line is considered not part of any code example and
        /// should be formatted as if the [`Print`] action were returned.
        /// Otherwise, if there is no line, then either one does not exist
        /// or it is part of another code example and should be treated as a
        /// [`Kept`] action.
        original: Option<InputDocstringLine<'src>>,
    },
    /// This occurs when adding a line to an existing code example
    /// results in that code example becoming invalid. In this case,
    /// we don't want to treat it as a code example, but instead write
    /// back the lines to the docstring unchanged.
    #[allow(dead_code)] // FIXME: remove when reStructuredText support is added
    Reset {
        /// The lines of code that we collected but should be printed back to
        /// the docstring as-is and not formatted.
        code: Vec<CodeExampleLine<'src>>,
        /// The line that was added and triggered this reset to occur. It
        /// should be written back to the docstring as-is after the code lines.
        original: InputDocstringLine<'src>,
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
    options: &crate::PyFormatOptions,
    docstring_quote_style: QuoteStyle,
    source: &str,
) -> Result<Printed, FormatModuleError> {
    use ruff_python_parser::AsMode;

    let source_type = options.source_type();
    let (tokens, comment_ranges) = ruff_python_index::tokens_and_ranges(source, source_type)?;
    let module =
        ruff_python_parser::parse_ok_tokens(tokens, source, source_type.as_mode(), "<filename>")?;
    let source_code = ruff_formatter::SourceCode::new(source);
    let comments = crate::Comments::from_ast(&module, source_code, &comment_ranges);
    let locator = Locator::new(source);

    let ctx = PyFormatContext::new(options.clone(), locator.contents(), comments)
        .in_docstring(docstring_quote_style);
    let formatted = crate::format!(ctx, [module.format()])?;
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
    trim_end.ends_with(normalized.quotes.style.as_char())
        || trim_end.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// For docstring indentation, black counts spaces as 1 and tabs by increasing the indentation up
/// to the next multiple of 8. This is effectively a port of
/// [`str.expandtabs`](https://docs.python.org/3/library/stdtypes.html#str.expandtabs),
/// which black [calls with the default tab width of 8](https://github.com/psf/black/blob/c36e468794f9256d5e922c399240d49782ba04f1/src/black/strings.py#L61).
fn indentation_length(line: &str) -> TextSize {
    let mut indentation = 0u32;
    for char in line.chars() {
        if char == '\t' {
            // Pad to the next multiple of tab_width
            indentation += 8 - (indentation.rem_euclid(8));
        } else if char.is_whitespace() {
            indentation += u32::from(char.text_len());
        } else {
            break;
        }
    }
    TextSize::new(indentation)
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextSize;

    use super::indentation_length;

    #[test]
    fn test_indentation_like_black() {
        assert_eq!(indentation_length("\t \t  \t"), TextSize::new(24));
        assert_eq!(indentation_length("\t        \t"), TextSize::new(24));
        assert_eq!(indentation_length("\t\t\t"), TextSize::new(24));
        assert_eq!(indentation_length("    "), TextSize::new(4));
    }
}
