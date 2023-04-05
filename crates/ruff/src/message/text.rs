use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Location, Message};
use crate::registry::AsRule;
use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use colored::Colorize;
use ruff_diagnostics::DiagnosticKind;
use ruff_python_ast::source_code::OneIndexed;
use ruff_python_ast::types::Range;
use ruff_text_size::TextRange;
use std::fmt::{Display, Formatter};
use std::io::Write;

#[derive(Default)]
pub struct TextEmitter {
    show_fix_status: bool,
}

impl TextEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.show_fix_status = show_fix_status;
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
                path = relativize_path(&message.filename).bold(),
                sep = ":".cyan(),
            )?;

            // Check if we're working on a jupyter notebook and translate positions with cell accordingly
            let (row, col) = if let Some(jupyter_index) = context.jupyter_index(&message.filename) {
                write!(
                    writer,
                    "cell {cell}{sep}",
                    cell = jupyter_index.row_to_cell[message.location.row()],
                    sep = ":".cyan(),
                )?;

                (
                    jupyter_index.row_to_row_in_cell[message.location.row()] as usize,
                    message.location.column(),
                )
            } else {
                (message.location.row(), message.location.column())
            };

            writeln!(
                writer,
                "{row}{sep}{col}{sep} {code_and_body}",
                sep = ":".cyan(),
                code_and_body = RuleCodeAndBody {
                    message_kind: &message.kind,
                    show_fix_status: self.show_fix_status
                }
            )?;

            if message.source.is_some() {
                writeln!(writer, "{}", MessageCodeFrame { message })?;
            }
        }

        Ok(())
    }
}

pub(super) struct RuleCodeAndBody<'a> {
    pub message_kind: &'a DiagnosticKind,
    pub show_fix_status: bool,
}

impl Display for RuleCodeAndBody<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.show_fix_status && self.message_kind.fixable {
            write!(
                f,
                "{code} {autofix}{body}",
                code = self
                    .message_kind
                    .rule()
                    .noqa_code()
                    .to_string()
                    .red()
                    .bold(),
                autofix = format_args!("[{}] ", "*".cyan()),
                body = self.message_kind.body,
            )
        } else {
            write!(
                f,
                "{code} {body}",
                code = self
                    .message_kind
                    .rule()
                    .noqa_code()
                    .to_string()
                    .red()
                    .bold(),
                body = self.message_kind.body,
            )
        }
    }
}

pub(super) struct MessageCodeFrame<'a> {
    pub message: &'a Message,
}

impl Display for MessageCodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Message {
            kind,
            source,
            location,
            end_location,
            ..
        } = self.message;

        if let Some(source_code) = source {
            let suggestion = kind.suggestion.as_deref();
            let footer = if suggestion.is_some() {
                vec![Annotation {
                    id: None,
                    label: suggestion,
                    annotation_type: AnnotationType::Help,
                }]
            } else {
                Vec::new()
            };

            let source_code_start =
                source_code.line_start(OneIndexed::new(location.row()).unwrap());

            let source_code_end = source_code.line_start(
                OneIndexed::new(
                    end_location
                        .row()
                        .saturating_add(1)
                        .min(source_code.line_count() + 1),
                )
                .unwrap(),
            );

            let source_text =
                &source_code.text()[TextRange::new(source_code_start, source_code_end)];

            let content_range = source_code.text_range(Range::new(
                // Subtract 1 because message column indices are 1 based but the index columns are 1 based.
                Location::new(location.row(), location.column().saturating_sub(1)),
                Location::new(end_location.row(), end_location.column().saturating_sub(1)),
            ));

            let annotation_length = &source_text[content_range - source_code_start]
                .chars()
                .count();

            let label = kind.rule().noqa_code().to_string();

            let snippet = Snippet {
                title: None,
                slices: vec![Slice {
                    source: source_text,
                    line_start: location.row(),
                    annotations: vec![SourceAnnotation {
                        label: &label,
                        annotation_type: AnnotationType::Error,
                        range: (
                            location.column() - 1,
                            location.column() + annotation_length - 1,
                        ),
                    }],
                    // The origin (file name, line number, and column number) is already encoded
                    // in the `label`.
                    origin: None,
                    fold: false,
                }],
                footer,
                opt: FormatOptions {
                    #[cfg(test)]
                    color: false,
                    #[cfg(not(test))]
                    color: colored::control::SHOULD_COLORIZE.should_colorize(),
                    ..FormatOptions::default()
                },
            };

            writeln!(f, "{message}", message = DisplayList::from(snippet))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::TextEmitter;
    use insta::assert_snapshot;

    #[test]
    fn default() {
        let mut emitter = TextEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn fix_status() {
        let mut emitter = TextEmitter::default().with_show_fix_status(true);
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
