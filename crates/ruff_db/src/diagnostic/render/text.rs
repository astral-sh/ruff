use std::borrow::Cow;

use colored::Colorize;

use ruff_annotate_snippets::{Level, Renderer, Snippet};
use ruff_notebook::NotebookIndex;
use ruff_source_file::OneIndexed;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::diagnostic::{
    Diagnostic, SecondaryCode,
    line_width::{IndentWidth, LineWidthBuilder},
};

use super::{FileResolver, UnsafeFixes};

pub(super) struct RuleCodeAndBody<'a> {
    pub(crate) message: &'a Diagnostic,
    pub(crate) show_fix_status: bool,
    pub(crate) unsafe_fixes: UnsafeFixes,
}

impl std::fmt::Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.show_fix_status {
            if let Some(fix) = self.message.fix() {
                // Do not display an indicator for inapplicable fixes
                if fix.applies(self.unsafe_fixes.required_applicability()) {
                    if let Some(code) = self.message.secondary_code() {
                        write!(f, "{} ", code.red().bold())?;
                    }
                    return write!(
                        f,
                        "{fix}{body}",
                        fix = format_args!("[{}] ", "*".cyan()),
                        body = self.message.body(),
                    );
                }
            }
        }

        if let Some(code) = self.message.secondary_code() {
            write!(
                f,
                "{code} {body}",
                code = code.red().bold(),
                body = self.message.body(),
            )
        } else {
            f.write_str(self.message.body())
        }
    }
}

pub(super) struct MessageCodeFrame<'a> {
    pub(crate) message: &'a Diagnostic,
    pub(crate) notebook_index: Option<&'a NotebookIndex>,
    pub(crate) resolver: &'a dyn FileResolver,
}

impl std::fmt::Display for MessageCodeFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(span) = self.message.primary_span_ref() else {
            return Ok(());
        };

        let suggestion = self.message.suggestion();
        let footers = if let Some(suggestion) = suggestion {
            vec![Level::Help.title(suggestion)]
        } else {
            Vec::new()
        };
        let file = span.file();
        let diagnostic_source = file.diagnostic_source(self.resolver);
        let source_code = diagnostic_source.as_source_code();

        let content_start_index = source_code.line_index(self.message.expect_range().start());
        let mut start_index = content_start_index.saturating_sub(2);

        // If we're working with a Jupyter Notebook, skip the lines which are
        // outside of the cell containing the diagnostic.
        if let Some(index) = self.notebook_index {
            let content_start_cell = index.cell(content_start_index).unwrap_or(OneIndexed::MIN);
            while start_index < content_start_index {
                if index.cell(start_index).unwrap_or(OneIndexed::MIN) == content_start_cell {
                    break;
                }
                start_index = start_index.saturating_add(1);
            }
        }

        // Trim leading empty lines.
        while start_index < content_start_index {
            if !source_code.line_text(start_index).trim().is_empty() {
                break;
            }
            start_index = start_index.saturating_add(1);
        }

        let content_end_index = source_code.line_index(self.message.expect_range().end());
        let mut end_index = content_end_index
            .saturating_add(2)
            .min(OneIndexed::from_zero_indexed(source_code.line_count()));

        // If we're working with a Jupyter Notebook, skip the lines which are
        // outside of the cell containing the diagnostic.
        if let Some(index) = self.notebook_index {
            let content_end_cell = index.cell(content_end_index).unwrap_or(OneIndexed::MIN);
            while end_index > content_end_index {
                if index.cell(end_index).unwrap_or(OneIndexed::MIN) == content_end_cell {
                    break;
                }
                end_index = end_index.saturating_sub(1);
            }
        }

        // Trim trailing empty lines.
        while end_index > content_end_index {
            if !source_code.line_text(end_index).trim().is_empty() {
                break;
            }

            end_index = end_index.saturating_sub(1);
        }

        let start_offset = source_code.line_start(start_index);
        let end_offset = source_code.line_end(end_index);

        let source = replace_whitespace_and_unprintable(
            source_code.slice(TextRange::new(start_offset, end_offset)),
            self.message.expect_range() - start_offset,
        )
        .fix_up_empty_spans_after_line_terminator();

        let label = self
            .message
            .secondary_code()
            .map(SecondaryCode::as_str)
            .unwrap_or_default();

        let line_start = self.notebook_index.map_or_else(
            || start_index.get(),
            |notebook_index| {
                notebook_index
                    .cell_row(start_index)
                    .unwrap_or(OneIndexed::MIN)
                    .get()
            },
        );

        let span = usize::from(source.annotation_range.start())
            ..usize::from(source.annotation_range.end());
        let annotation = Level::Error.span(span).label(label);
        let snippet = Snippet::source(&source.text)
            .line_start(line_start)
            .annotation(annotation)
            .fold(false);
        let message = Level::None.title("").snippet(snippet).footers(footers);

        let renderer = if !cfg!(test) && colored::control::SHOULD_COLORIZE.should_colorize() {
            Renderer::styled()
        } else {
            Renderer::plain()
        }
        .cut_indicator("…");
        let rendered = renderer.render(message);
        writeln!(f, "{rendered}")
    }
}

/// Given some source code and an annotation range, this routine replaces
/// tabs with ASCII whitespace, and unprintable characters with printable
/// representations of them.
///
/// The source code returned has an annotation that is updated to reflect
/// changes made to the source code (if any).
fn replace_whitespace_and_unprintable(source: &str, annotation_range: TextRange) -> SourceCode {
    let mut result = String::new();
    let mut last_end = 0;
    let mut range = annotation_range;
    let mut line_width = LineWidthBuilder::new(IndentWidth::default());

    // Updates the range given by the caller whenever a single byte (at
    // `index` in `source`) is replaced with `len` bytes.
    //
    // When the index occurs before the start of the range, the range is
    // offset by `len`. When the range occurs after or at the start but before
    // the end, then the end of the range only is offset by `len`.
    let mut update_range = |index, len| {
        if index < usize::from(annotation_range.start()) {
            range += TextSize::new(len - 1);
        } else if index < usize::from(annotation_range.end()) {
            range = range.add_end(TextSize::new(len - 1));
        }
    };

    // If `c` is an unprintable character, then this returns a printable
    // representation of it (using a fancier Unicode codepoint).
    let unprintable_replacement = |c: char| -> Option<char> {
        match c {
            '\x07' => Some('␇'),
            '\x08' => Some('␈'),
            '\x1b' => Some('␛'),
            '\x7f' => Some('␡'),
            _ => None,
        }
    };

    for (index, c) in source.char_indices() {
        let old_width = line_width.get();
        line_width = line_width.add_char(c);

        if matches!(c, '\t') {
            let tab_width = u32::try_from(line_width.get() - old_width)
                .expect("small width because of tab size");
            result.push_str(&source[last_end..index]);
            for _ in 0..tab_width {
                result.push(' ');
            }
            last_end = index + 1;
            update_range(index, tab_width);
        } else if let Some(printable) = unprintable_replacement(c) {
            result.push_str(&source[last_end..index]);
            result.push(printable);
            last_end = index + 1;

            let len = printable.text_len().to_u32();
            update_range(index, len);
        }
    }

    // No tabs or unprintable chars
    if result.is_empty() {
        SourceCode {
            annotation_range,
            text: Cow::Borrowed(source),
        }
    } else {
        result.push_str(&source[last_end..]);
        SourceCode {
            annotation_range: range,
            text: Cow::Owned(result),
        }
    }
}

struct SourceCode<'a> {
    text: Cow<'a, str>,
    annotation_range: TextRange,
}

impl<'a> SourceCode<'a> {
    /// This attempts to "fix up" the span on `SourceCode` in the case where
    /// it's an empty span immediately following a line terminator.
    ///
    /// At present, `annotate-snippets` (both upstream and our vendored copy)
    /// will render annotations of such spans to point to the space immediately
    /// following the previous line. But ideally, this should point to the space
    /// immediately preceding the next line.
    ///
    /// After attempting to fix `annotate-snippets` and giving up after a couple
    /// hours, this routine takes a different tact: it adjusts the span to be
    /// non-empty and it will cover the first codepoint of the following line.
    /// This forces `annotate-snippets` to point to the right place.
    ///
    /// See also: <https://github.com/astral-sh/ruff/issues/15509>
    fn fix_up_empty_spans_after_line_terminator(self) -> SourceCode<'a> {
        if !self.annotation_range.is_empty()
            || self.annotation_range.start() == TextSize::from(0)
            || self.annotation_range.start() >= self.text.text_len()
        {
            return self;
        }
        if self.text.as_bytes()[self.annotation_range.start().to_usize() - 1] != b'\n' {
            return self;
        }
        let start = self.annotation_range.start();
        let end = ceil_char_boundary(&self.text, start + TextSize::from(1));
        SourceCode {
            annotation_range: TextRange::new(start, end),
            ..self
        }
    }
}

/// Finds the closest [`TextSize`] not less than the offset given for which
/// `is_char_boundary` is `true`. Unless the offset given is greater than
/// the length of the underlying contents, in which case, the length of the
/// contents is returned.
///
/// Can be replaced with `str::ceil_char_boundary` once it's stable.
///
/// # Examples
///
/// From `std`:
///
/// ```
/// use ruff_text_size::{Ranged, TextSize};
/// use ruff_linter::Locator;
///
/// let locator = Locator::new("❤️🧡💛💚💙💜");
/// assert_eq!(locator.text_len(), TextSize::from(26));
/// assert!(!locator.contents().is_char_boundary(13));
///
/// let closest = locator.ceil_char_boundary(TextSize::from(13));
/// assert_eq!(closest, TextSize::from(14));
/// assert_eq!(&locator.contents()[..closest.to_usize()], "❤️🧡💛");
/// ```
///
/// Additional examples:
///
/// ```
/// use ruff_text_size::{Ranged, TextRange, TextSize};
/// use ruff_linter::Locator;
///
/// let locator = Locator::new("Hello");
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(0)),
///     TextSize::from(0)
/// );
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(5)),
///     TextSize::from(5)
/// );
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(6)),
///     TextSize::from(5)
/// );
///
/// let locator = Locator::new("α");
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(0)),
///     TextSize::from(0)
/// );
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(1)),
///     TextSize::from(2)
/// );
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(2)),
///     TextSize::from(2)
/// );
///
/// assert_eq!(
///     locator.ceil_char_boundary(TextSize::from(3)),
///     TextSize::from(2)
/// );
/// ```
pub fn ceil_char_boundary(text: &str, offset: TextSize) -> TextSize {
    let upper_bound = offset
        .to_u32()
        .saturating_add(4)
        .min(text.text_len().to_u32());
    (offset.to_u32()..upper_bound)
        .map(TextSize::from)
        .find(|offset| text.is_char_boundary(offset.to_usize()))
        .unwrap_or_else(|| TextSize::from(upper_bound))
}
