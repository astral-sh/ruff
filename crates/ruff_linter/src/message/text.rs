use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io::Write;

use bitflags::bitflags;
use colored::Colorize;
use ruff_annotate_snippets::{Level, Renderer, Snippet};

use ruff_notebook::NotebookIndex;
use ruff_source_file::{OneIndexed, SourceLocation};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::fs::relativize_path;
use crate::line_width::{IndentWidth, LineWidthBuilder};
use crate::message::diff::Diff;
use crate::message::{Emitter, EmitterContext, Message};
use crate::settings::types::UnsafeFixes;
use crate::Locator;

bitflags! {
    #[derive(Default)]
    struct EmitterFlags: u8 {
        /// Whether to show the fix status of a diagnostic.
        const SHOW_FIX_STATUS   = 1 << 0;
        /// Whether to show the diff of a fix, for diagnostics that have a fix.
        const SHOW_FIX_DIFF     = 1 << 1;
        /// Whether to show the source code of a diagnostic.
        const SHOW_SOURCE       = 1 << 2;
    }
}

#[derive(Default)]
pub struct TextEmitter {
    flags: EmitterFlags,
    unsafe_fixes: UnsafeFixes,
}

impl TextEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.flags
            .set(EmitterFlags::SHOW_FIX_STATUS, show_fix_status);
        self
    }

    #[must_use]
    pub fn with_show_fix_diff(mut self, show_fix_diff: bool) -> Self {
        self.flags.set(EmitterFlags::SHOW_FIX_DIFF, show_fix_diff);
        self
    }

    #[must_use]
    pub fn with_show_source(mut self, show_source: bool) -> Self {
        self.flags.set(EmitterFlags::SHOW_SOURCE, show_source);
        self
    }

    #[must_use]
    pub fn with_unsafe_fixes(mut self, unsafe_fixes: UnsafeFixes) -> Self {
        self.unsafe_fixes = unsafe_fixes;
        self
    }
}

impl Emitter for TextEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for message in messages {
            write!(
                writer,
                "{path}{sep}",
                path = relativize_path(message.filename()).bold(),
                sep = ":".cyan(),
            )?;

            let start_location = message.compute_start_location();
            let notebook_index = context.notebook_index(message.filename());

            // Check if we're working on a jupyter notebook and translate positions with cell accordingly
            let diagnostic_location = if let Some(notebook_index) = notebook_index {
                write!(
                    writer,
                    "cell {cell}{sep}",
                    cell = notebook_index
                        .cell(start_location.row)
                        .unwrap_or(OneIndexed::MIN),
                    sep = ":".cyan(),
                )?;

                SourceLocation {
                    row: notebook_index
                        .cell_row(start_location.row)
                        .unwrap_or(OneIndexed::MIN),
                    column: start_location.column,
                }
            } else {
                start_location
            };

            writeln!(
                writer,
                "{row}{sep}{col}{sep} {code_and_body}",
                row = diagnostic_location.row,
                col = diagnostic_location.column,
                sep = ":".cyan(),
                code_and_body = RuleCodeAndBody {
                    message,
                    show_fix_status: self.flags.intersects(EmitterFlags::SHOW_FIX_STATUS),
                    unsafe_fixes: self.unsafe_fixes,
                }
            )?;

            if self.flags.intersects(EmitterFlags::SHOW_SOURCE) {
                // The `0..0` range is used to highlight file-level diagnostics.
                if message.range() != TextRange::default() {
                    writeln!(
                        writer,
                        "{}",
                        MessageCodeFrame {
                            message,
                            notebook_index
                        }
                    )?;
                }
            }

            if self.flags.intersects(EmitterFlags::SHOW_FIX_DIFF) {
                if let Some(diff) = Diff::from_message(message) {
                    writeln!(writer, "{diff}")?;
                }
            }
        }

        Ok(())
    }
}

pub(super) struct RuleCodeAndBody<'a> {
    pub(crate) message: &'a Message,
    pub(crate) show_fix_status: bool,
    pub(crate) unsafe_fixes: UnsafeFixes,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.show_fix_status {
            if let Some(fix) = self.message.fix() {
                // Do not display an indicator for inapplicable fixes
                if fix.applies(self.unsafe_fixes.required_applicability()) {
                    if let Some(rule) = self.message.rule() {
                        write!(f, "{} ", rule.noqa_code().to_string().red().bold())?;
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

        if let Some(rule) = self.message.rule() {
            write!(
                f,
                "{code} {body}",
                code = rule.noqa_code().to_string().red().bold(),
                body = self.message.body(),
            )
        } else {
            f.write_str(self.message.body())
        }
    }
}

pub(super) struct MessageCodeFrame<'a> {
    pub(crate) message: &'a Message,
    pub(crate) notebook_index: Option<&'a NotebookIndex>,
}

impl Display for MessageCodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let suggestion = self.message.suggestion();
        let footers = if let Some(suggestion) = suggestion {
            vec![Level::Help.title(suggestion)]
        } else {
            Vec::new()
        };

        let source_code = self.message.source_file().to_source_code();

        let content_start_index = source_code.line_index(self.message.start());
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

        let content_end_index = source_code.line_index(self.message.end());
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
            self.message.range() - start_offset,
        )
        .fix_up_empty_spans_after_line_terminator();

        let label = self
            .message
            .rule()
            .map_or_else(String::new, |rule| rule.noqa_code().to_string());

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
        let annotation = Level::Error.span(span).label(&label);
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
        let locator = Locator::new(&self.text);
        let start = self.annotation_range.start();
        let end = locator.ceil_char_boundary(start + TextSize::from(1));
        SourceCode {
            annotation_range: TextRange::new(start, end),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_messages,
        create_notebook_messages, create_syntax_error_messages,
    };
    use crate::message::TextEmitter;
    use crate::settings::types::UnsafeFixes;

    #[test]
    fn default() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status_unsafe() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = TextEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let (messages, notebook_indexes) = create_notebook_messages();
        let content = capture_emitter_notebook_output(&mut emitter, &messages, &notebook_indexes);

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = TextEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());

        assert_snapshot!(content);
    }
}
