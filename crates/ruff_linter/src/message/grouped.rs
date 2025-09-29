use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::num::NonZeroUsize;

use colored::Colorize;

use ruff_db::diagnostic::Diagnostic;
use ruff_diagnostics::Applicability;
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed};

use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext};

pub struct GroupedEmitter {
    show_fix_status: bool,
    applicability: Applicability,
}

impl Default for GroupedEmitter {
    fn default() -> Self {
        Self {
            show_fix_status: false,
            applicability: Applicability::Safe,
        }
    }
}

impl GroupedEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.show_fix_status = show_fix_status;
        self
    }

    #[must_use]
    pub fn with_applicability(mut self, applicability: Applicability) -> Self {
        self.applicability = applicability;
        self
    }
}

impl Emitter for GroupedEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for (filename, messages) in group_diagnostics_by_filename(diagnostics) {
            // Compute the maximum number of digits in the row and column, for messages in
            // this file.

            let mut max_row_length = OneIndexed::MIN;
            let mut max_column_length = OneIndexed::MIN;

            for message in &messages {
                max_row_length = max_row_length.max(message.start_location.line);
                max_column_length = max_column_length.max(message.start_location.column);
            }

            let row_length = max_row_length.digits();
            let column_length = max_column_length.digits();

            // Print the filename.
            writeln!(writer, "{}:", relativize_path(&*filename).underline())?;

            // Print each message.
            for message in messages {
                write!(
                    writer,
                    "{}",
                    DisplayGroupedMessage {
                        notebook_index: context.notebook_index(&message.expect_ruff_filename()),
                        message,
                        show_fix_status: self.show_fix_status,
                        applicability: self.applicability,
                        row_length,
                        column_length,
                    }
                )?;
            }

            // Print a blank line between files.
            writeln!(writer)?;
        }

        Ok(())
    }
}

struct MessageWithLocation<'a> {
    message: &'a Diagnostic,
    start_location: LineColumn,
}

impl std::ops::Deref for MessageWithLocation<'_> {
    type Target = Diagnostic;

    fn deref(&self) -> &Self::Target {
        self.message
    }
}

fn group_diagnostics_by_filename(
    diagnostics: &[Diagnostic],
) -> BTreeMap<String, Vec<MessageWithLocation<'_>>> {
    let mut grouped_messages = BTreeMap::default();
    for diagnostic in diagnostics {
        grouped_messages
            .entry(diagnostic.expect_ruff_filename())
            .or_insert_with(Vec::new)
            .push(MessageWithLocation {
                message: diagnostic,
                start_location: diagnostic.ruff_start_location().unwrap_or_default(),
            });
    }
    grouped_messages
}

struct DisplayGroupedMessage<'a> {
    message: MessageWithLocation<'a>,
    show_fix_status: bool,
    applicability: Applicability,
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
            row_padding = " ".repeat(self.row_length.get() - start_location.line.digits().get())
        )?;

        // Check if we're working on a jupyter notebook and translate positions with cell accordingly
        let (row, col) = if let Some(jupyter_index) = self.notebook_index {
            write!(
                f,
                "cell {cell}{sep}",
                cell = jupyter_index
                    .cell(start_location.line)
                    .unwrap_or(OneIndexed::MIN),
                sep = ":".cyan()
            )?;
            (
                jupyter_index
                    .cell_row(start_location.line)
                    .unwrap_or(OneIndexed::MIN),
                start_location.column,
            )
        } else {
            (start_location.line, start_location.column)
        };

        writeln!(
            f,
            "{row}{sep}{col}{col_padding} {code_and_body}",
            sep = ":".cyan(),
            col_padding =
                " ".repeat(self.column_length.get() - start_location.column.digits().get()),
            code_and_body = RuleCodeAndBody {
                message,
                show_fix_status: self.show_fix_status,
                applicability: self.applicability
            },
        )?;

        Ok(())
    }
}

pub(super) struct RuleCodeAndBody<'a> {
    pub(crate) message: &'a Diagnostic,
    pub(crate) show_fix_status: bool,
    pub(crate) applicability: Applicability,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.show_fix_status {
            if let Some(fix) = self.message.fix() {
                // Do not display an indicator for inapplicable fixes
                if fix.applies(self.applicability) {
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
            write!(
                f,
                "{code}: {body}",
                code = self.message.id().as_str().red().bold(),
                body = self.message.body(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use ruff_diagnostics::Applicability;

    use crate::message::GroupedEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn default() {
        let mut emitter = GroupedEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = GroupedEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = GroupedEmitter::default().with_show_fix_status(true);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status_unsafe() {
        let mut emitter = GroupedEmitter::default()
            .with_show_fix_status(true)
            .with_applicability(Applicability::Unsafe);
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }
}
