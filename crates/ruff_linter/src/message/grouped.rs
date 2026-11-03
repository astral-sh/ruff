use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;

use colored::Colorize;

use ruff_db::diagnostic::render::Renderer;
use ruff_db::diagnostic::{Diagnostic, DisplayDiagnosticConfig, FileResolver};
use ruff_diagnostics::Applicability;
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed};

use crate::fs::relativize_path;

pub(crate) struct GroupedRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> GroupedRenderer<'a> {
    pub(crate) fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }
}

impl Renderer for GroupedRenderer<'_> {
    fn render(&self, f: &mut std::fmt::Formatter, diagnostics: &[Diagnostic]) -> std::fmt::Result {
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
            writeln!(f, "{}:", relativize_path(&*filename).underline())?;

            // Print each message.
            for message in messages {
                write!(
                    f,
                    "{}",
                    DisplayGroupedMessage {
                        notebook_index: self
                            .resolver
                            .notebook_index(message.expect_primary_span().file())
                            .as_ref(),
                        message,
                        show_fix_status: self.config.show_fix_status(),
                        applicability: self.config.fix_applicability(),
                        row_length,
                        column_length,
                        preview: self.config.preview_enabled(),
                        prefer_rule_codes: self.config.is_prefer_rule_codes_enabled(),
                    }
                )?;
            }

            // Print a blank line between files.
            writeln!(f)?;
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
    preview: bool,
    prefer_rule_codes: bool,
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
                applicability: self.applicability,
                preview: self.preview,
                prefer_rule_codes: self.prefer_rule_codes,
            },
        )?;

        Ok(())
    }
}

pub(super) struct RuleCodeAndBody<'a> {
    message: &'a Diagnostic,
    show_fix_status: bool,
    applicability: Applicability,
    preview: bool,
    prefer_rule_codes: bool,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let use_name = self.preview && !self.prefer_rule_codes;
        if self.show_fix_status {
            if let Some(fix) = self.message.fix() {
                // Do not display an indicator for inapplicable fixes
                if fix.applies(self.applicability) {
                    let code = if use_name {
                        self.message.id().as_str()
                    } else {
                        self.message.secondary_code_or_id()
                    };
                    write!(f, "{} ", code.red().bold())?;
                    return write!(
                        f,
                        "{fix}{body}",
                        fix = format_args!("[{}] ", "*".cyan()),
                        body = self.message.concise_message(),
                    );
                }
            }
        }

        if !use_name && let Some(code) = self.message.secondary_code() {
            write!(
                f,
                "{code} {body}",
                code = code.red().bold(),
                body = self.message.concise_message(),
            )
        } else {
            write!(
                f,
                "{code}: {body}",
                code = self.message.id().as_str().red().bold(),
                body = self.message.concise_message(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use rustc_hash::FxHashMap;

    use ruff_db::diagnostic::DisplayDiagnosticConfig;
    use ruff_db::diagnostic::render::Renderer;
    use ruff_diagnostics::Applicability;

    use crate::message::tests::{create_diagnostics, create_syntax_error_diagnostics};
    use crate::message::{GroupedRenderer, RenderContext};

    #[test]
    fn default() {
        assert_snapshot!(
            GroupedRenderer::new(
                &RenderContext::new(&FxHashMap::default()),
                &DisplayDiagnosticConfig::new("ruff"),
            )
            .display(&create_diagnostics())
        );
    }

    #[test]
    fn syntax_errors() {
        assert_snapshot!(
            GroupedRenderer::new(
                &RenderContext::new(&FxHashMap::default()),
                &DisplayDiagnosticConfig::new("ruff"),
            )
            .display(&create_syntax_error_diagnostics())
        );
    }

    #[test]
    fn fix_status() {
        assert_snapshot!(
            GroupedRenderer::new(
                &RenderContext::new(&FxHashMap::default()),
                &DisplayDiagnosticConfig::new("ruff").with_show_fix_status(true),
            )
            .display(&create_diagnostics())
        );
    }

    #[test]
    fn fix_status_unsafe() {
        assert_snapshot!(
            GroupedRenderer::new(
                &RenderContext::new(&FxHashMap::default()),
                &DisplayDiagnosticConfig::new("ruff")
                    .with_show_fix_status(true)
                    .with_fix_applicability(Applicability::Unsafe),
            )
            .display(&create_diagnostics())
        );
    }
}
