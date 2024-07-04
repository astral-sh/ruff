use std::fmt::{Display, Formatter};
use std::io::Write;
use std::num::NonZeroUsize;

use colored::Colorize;

use ruff_notebook::NotebookIndex;
use ruff_source_file::OneIndexed;

use crate::fs::relativize_path;
use crate::message::diff::calculate_print_width;
use crate::message::text::{MessageCodeFrame, RuleCodeAndBody};
use crate::message::{
    group_messages_by_filename, Emitter, EmitterContext, Message, MessageWithLocation,
};
use crate::settings::types::UnsafeFixes;

#[derive(Default)]
pub struct GroupedEmitter {
    show_fix_status: bool,
    show_source: bool,
    unsafe_fixes: UnsafeFixes,
}

impl GroupedEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.show_fix_status = show_fix_status;
        self
    }

    #[must_use]
    pub fn with_show_source(mut self, show_source: bool) -> Self {
        self.show_source = show_source;
        self
    }

    #[must_use]
    pub fn with_unsafe_fixes(mut self, unsafe_fixes: UnsafeFixes) -> Self {
        self.unsafe_fixes = unsafe_fixes;
        self
    }
}

impl Emitter for GroupedEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for (filename, messages) in group_messages_by_filename(messages) {
            // Compute the maximum number of digits in the row and column, for messages in
            // this file.

            let mut max_row_length = OneIndexed::MIN;
            let mut max_column_length = OneIndexed::MIN;

            for message in &messages {
                max_row_length = max_row_length.max(message.start_location.row);
                max_column_length = max_column_length.max(message.start_location.column);
            }

            let row_length = calculate_print_width(max_row_length);
            let column_length = calculate_print_width(max_column_length);

            // Print the filename.
            writeln!(writer, "{}:", relativize_path(filename).underline())?;

            // Print each message.
            for message in messages {
                write!(
                    writer,
                    "{}",
                    DisplayGroupedMessage {
                        notebook_index: context.notebook_index(message.filename()),
                        message,
                        show_fix_status: self.show_fix_status,
                        unsafe_fixes: self.unsafe_fixes,
                        show_source: self.show_source,
                        row_length,
                        column_length,
                    }
                )?;
            }

            // Print a blank line between files, unless we're showing the source, in which case
            // we'll have already printed a blank line between messages.
            if !self.show_source {
                writeln!(writer)?;
            }
        }

        Ok(())
    }
}

struct DisplayGroupedMessage<'a> {
    message: MessageWithLocation<'a>,
    show_fix_status: bool,
    unsafe_fixes: UnsafeFixes,
    show_source: bool,
    row_length: NonZeroUsize,
    column_length: NonZeroUsize,
    notebook_index: Option<&'a NotebookIndex>,
}

impl Display for DisplayGroupedMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let MessageWithLocation {
            message,
            start_location,
        } = &self.message;

        write!(
            f,
            "  {row_padding}",
            row_padding =
                " ".repeat(self.row_length.get() - calculate_print_width(start_location.row).get())
        )?;

        // Check if we're working on a jupyter notebook and translate positions with cell accordingly
        let (row, col) = if let Some(jupyter_index) = self.notebook_index {
            write!(
                f,
                "cell {cell}{sep}",
                cell = jupyter_index
                    .cell(start_location.row)
                    .unwrap_or(OneIndexed::MIN),
                sep = ":".cyan()
            )?;
            (
                jupyter_index
                    .cell_row(start_location.row)
                    .unwrap_or(OneIndexed::MIN),
                start_location.column,
            )
        } else {
            (start_location.row, start_location.column)
        };

        writeln!(
            f,
            "{row}{sep}{col}{col_padding} {code_and_body}",
            sep = ":".cyan(),
            col_padding = " ".repeat(
                self.column_length.get() - calculate_print_width(start_location.column).get()
            ),
            code_and_body = RuleCodeAndBody {
                message,
                show_fix_status: self.show_fix_status,
                unsafe_fixes: self.unsafe_fixes
            },
        )?;

        if self.show_source {
            use std::fmt::Write;
            let mut padded = PadAdapter::new(f);
            writeln!(
                padded,
                "{}",
                MessageCodeFrame {
                    message,
                    notebook_index: self.notebook_index
                }
            )?;
        }

        Ok(())
    }
}

/// Adapter that adds a '  ' at the start of every line without the need to copy the string.
/// Inspired by Rust's `debug_struct()` internal implementation that also uses a `PadAdapter`.
struct PadAdapter<'buf> {
    buf: &'buf mut (dyn std::fmt::Write + 'buf),
    on_newline: bool,
}

impl<'buf> PadAdapter<'buf> {
    fn new(buf: &'buf mut (dyn std::fmt::Write + 'buf)) -> Self {
        Self {
            buf,
            on_newline: true,
        }
    }
}

impl std::fmt::Write for PadAdapter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for s in s.split_inclusive('\n') {
            if self.on_newline {
                self.buf.write_str("  ")?;
            }

            self.on_newline = s.ends_with('\n');
            self.buf.write_str(s)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{
        capture_emitter_output, create_messages, create_syntax_error_messages,
    };
    use crate::message::GroupedEmitter;
    use crate::settings::types::UnsafeFixes;

    #[test]
    fn default() {
        let mut emitter = GroupedEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = GroupedEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn show_source() {
        let mut emitter = GroupedEmitter::default().with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = GroupedEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status_unsafe() {
        let mut emitter = GroupedEmitter::default()
            .with_show_fix_status(true)
            .with_show_source(true)
            .with_unsafe_fixes(UnsafeFixes::Enabled);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
