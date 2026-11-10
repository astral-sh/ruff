use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;

use ruff_diagnostics::Applicability;
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed};

use crate::diagnostic::render::Renderer;
use crate::diagnostic::stylesheet::{DiagnosticStylesheet, fmt_styled};
use crate::diagnostic::{Diagnostic, DisplayDiagnosticConfig, FileResolver};

pub struct GroupedRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> GroupedRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }
}

impl Renderer for GroupedRenderer<'_> {
    fn render(&self, f: &mut std::fmt::Formatter, diagnostics: &[Diagnostic]) -> std::fmt::Result {
        let stylesheet = if self.config.color {
            DiagnosticStylesheet::styled()
        } else {
            DiagnosticStylesheet::plain()
        };
        for (filename, messages) in group_diagnostics_by_filename(diagnostics, self.resolver) {
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
            writeln!(f, "{}:", fmt_styled(filename, stylesheet.underline))?;

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
                        stylesheet: &stylesheet,
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

fn group_diagnostics_by_filename<'a>(
    diagnostics: &'a [Diagnostic],
    resolver: &'a dyn FileResolver,
) -> BTreeMap<Cow<'a, str>, Vec<MessageWithLocation<'a>>> {
    let mut grouped_messages = BTreeMap::default();
    for diagnostic in diagnostics {
        let span = diagnostic
            .primary_span_ref()
            .expect("Expected a primary span");
        let file = span.file();
        let start_location = span.range().map(|range| {
            file.diagnostic_source(resolver)
                .as_source_code()
                .line_column(range.start())
        });
        grouped_messages
            .entry(file.relative_path(resolver).to_string_lossy())
            .or_insert_with(Vec::new)
            .push(MessageWithLocation {
                message: diagnostic,
                start_location: start_location.unwrap_or_default(),
            });
    }
    grouped_messages
}

struct DisplayGroupedMessage<'a> {
    message: MessageWithLocation<'a>,
    stylesheet: &'a DiagnosticStylesheet,
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
                sep = fmt_styled(":", self.stylesheet.separator)
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
            sep = fmt_styled(":", self.stylesheet.separator),
            col_padding =
                " ".repeat(self.column_length.get() - start_location.column.digits().get()),
            code_and_body = RuleCodeAndBody {
                message,
                stylesheet: self.stylesheet,
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
    stylesheet: &'a DiagnosticStylesheet,
    show_fix_status: bool,
    applicability: Applicability,
    preview: bool,
    prefer_rule_codes: bool,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let use_name = self.preview && !self.prefer_rule_codes;
        if self.show_fix_status
            && let Some(fix) = self.message.fix()
        {
            // Do not display an indicator for inapplicable fixes
            if fix.applies(self.applicability) {
                let code = if use_name {
                    self.message.id().as_str()
                } else {
                    self.message.secondary_code_or_id()
                };
                write!(f, "{} ", fmt_styled(code, self.stylesheet.secondary_code))?;
                return write!(
                    f,
                    "{fix}{body}",
                    fix = format_args!("[{}] ", fmt_styled("*", self.stylesheet.separator)),
                    body = self.message.concise_message(),
                );
            }
        }

        if !use_name && let Some(code) = self.message.secondary_code() {
            write!(
                f,
                "{code} {body}",
                code = fmt_styled(code, self.stylesheet.secondary_code),
                body = self.message.concise_message(),
            )
        } else {
            write!(
                f,
                "{code}: {body}",
                code = fmt_styled(self.message.id().as_str(), self.stylesheet.secondary_code),
                body = self.message.concise_message(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_diagnostics::Applicability;

    use super::super::tests::{
        DiagnosticFormat, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn default() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @"
        fib.py:
           1:8  F401 `os` imported but unused
           6:5  F841 Local variable `x` is assigned to but never used
          12:16 F821 Undefined name `fibonaccii`

        undef.py:
          1:4 F821 Undefined name `a`
        ");
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Grouped);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @"
        syntax_errors.py:
          1:15 invalid-syntax: Expected one or more symbol names after import
          3:12 invalid-syntax: Expected ')', found newline
        ");
    }

    #[test]
    fn fix_status() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        env.show_fix_status(true);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @"
        fib.py:
           1:8  F401 `os` imported but unused
           6:5  F841 Local variable `x` is assigned to but never used
          12:16 F821 Undefined name `fibonaccii`

        undef.py:
          1:4 F821 Undefined name `a`
        ");
    }

    #[test]
    fn fix_status_unsafe() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Grouped);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::Unsafe);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @"
        fib.py:
           1:8  F401 [*] `os` imported but unused
           6:5  F841 [*] Local variable `x` is assigned to but never used
          12:16 F821 Undefined name `fibonaccii`

        undef.py:
          1:4 F821 Undefined name `a`
        ");
    }
}
