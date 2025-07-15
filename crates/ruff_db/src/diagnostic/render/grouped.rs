use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::ops::Deref;

use colored::Colorize;

use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed};

use crate::diagnostic::render::text::MessageCodeFrame;
use crate::diagnostic::render::{FileResolver, UnsafeFixes};
use crate::diagnostic::{Diagnostic, DisplayDiagnosticConfig};

use super::diff::calculate_print_width;
use super::text::RuleCodeAndBody;

pub(super) struct GroupedRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> GroupedRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        for (filename, messages) in group_diagnostics_by_filename(diagnostics, self.resolver) {
            // Compute the maximum number of digits in the row and column, for messages in
            // this file.

            let mut max_row_length = OneIndexed::MIN;
            let mut max_column_length = OneIndexed::MIN;

            for message in &messages {
                max_row_length = max_row_length.max(message.start_location.line);
                max_column_length = max_column_length.max(message.start_location.column);
            }

            let row_length = calculate_print_width(max_row_length);
            let column_length = calculate_print_width(max_column_length);

            let path = super::relativize_path(
                &*self.resolver.current_directory().to_string_lossy(),
                filename,
            );

            // Print the filename.
            writeln!(f, "{}:", path.underline())?;

            // Print each message.
            for message in messages {
                write!(
                    f,
                    "{}",
                    DisplayGroupedMessage {
                        notebook_index: message
                            .primary_span_ref()
                            .and_then(|span| self.resolver.notebook_index(span.file())),
                        message,
                        show_fix_status: self.config.show_fix_status,
                        unsafe_fixes: self.config.unsafe_fixes,
                        show_source: self.config.show_source,
                        row_length,
                        column_length,
                        resolver: self.resolver,
                    }
                )?;
            }

            // Print a blank line between files, unless we're showing the source, in which case
            // we'll have already printed a blank line between messages.
            if !self.config.show_source {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

struct DisplayGroupedMessage<'a> {
    message: DiagnosticWithLocation<'a>,
    show_fix_status: bool,
    unsafe_fixes: UnsafeFixes,
    show_source: bool,
    row_length: NonZeroUsize,
    column_length: NonZeroUsize,
    notebook_index: Option<NotebookIndex>,
    resolver: &'a dyn FileResolver,
}

impl Display for DisplayGroupedMessage<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let DiagnosticWithLocation {
            diagnostic: message,
            start_location,
        } = &self.message;

        write!(
            f,
            "  {row_padding}",
            row_padding = " "
                .repeat(self.row_length.get() - calculate_print_width(start_location.line).get())
        )?;

        // Check if we're working on a jupyter notebook and translate positions with cell accordingly
        let (row, col) = if let Some(jupyter_index) = &self.notebook_index {
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
                    notebook_index: self.notebook_index.as_ref(),
                    resolver: self.resolver,
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

pub(super) struct DiagnosticWithLocation<'a> {
    pub(super) diagnostic: &'a Diagnostic,
    pub(super) start_location: LineColumn,
}

impl Deref for DiagnosticWithLocation<'_> {
    type Target = Diagnostic;

    fn deref(&self) -> &Self::Target {
        self.diagnostic
    }
}

pub(super) fn group_diagnostics_by_filename<'a>(
    diagnostics: &'a [Diagnostic],
    resolver: &'a dyn FileResolver,
) -> BTreeMap<&'a str, Vec<DiagnosticWithLocation<'a>>> {
    let mut grouped_diagnostics = BTreeMap::default();
    for diagnostic in diagnostics {
        let (filename, start_location) = diagnostic
            .primary_span_ref()
            .map(|span| {
                let file = span.file();
                let start_location =
                    span.range()
                        .filter(|_| !resolver.is_notebook(file))
                        .map(|range| {
                            file.diagnostic_source(resolver)
                                .as_source_code()
                                .line_column(range.start())
                        });

                (file.path(resolver), start_location)
            })
            .unwrap_or_default();

        grouped_diagnostics
            .entry(filename)
            .or_insert_with(Vec::new)
            .push(DiagnosticWithLocation {
                diagnostic,
                start_location: start_location.unwrap_or_default(),
            });
    }
    grouped_diagnostics
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat, UnsafeFixes,
        render::tests::{TestEnvironment, create_diagnostics, create_syntax_error_diagnostics},
    };

    #[test]
    fn default() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Grouped);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn show_source() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        env.show_source(true);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn fix_status() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        env.show_source(true);
        env.show_fix_status(true);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn fix_status_unsafe() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        env.show_source(true);
        env.show_fix_status(true);
        env.unsafe_fixes(UnsafeFixes::Enabled);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Grouped);
        env.show_source(true);
        env.show_fix_status(true);
        env.unsafe_fixes(crate::diagnostic::UnsafeFixes::Enabled);

        let diag = env.err().build();

        insta::assert_snapshot!(
            env.render(&diag),
            @r"
        :
          1:1 main diagnostic message
        ",
        );
    }
}
