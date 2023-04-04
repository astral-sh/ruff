use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use colored::control::SHOULD_COLORIZE;
use colored::Colorize;
use ruff_diagnostics::DiagnosticKind;
use std::fmt::Display;
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

            if let Some(source) = &message.source {
                let suggestion = message.kind.suggestion.clone();
                let footer = if suggestion.is_some() {
                    vec![Annotation {
                        id: None,
                        label: suggestion.as_deref(),
                        annotation_type: AnnotationType::Help,
                    }]
                } else {
                    Vec::new()
                };

                let label = message.kind.rule().noqa_code().to_string();
                let snippet = Snippet {
                    title: None,
                    slices: vec![Slice {
                        source: &source.contents,
                        line_start: message.location.row(),
                        annotations: vec![SourceAnnotation {
                            label: &label,
                            annotation_type: AnnotationType::Error,
                            range: source.range,
                        }],
                        // The origin (file name, line number, and column number) is already encoded
                        // in the `label`.
                        origin: None,
                        fold: false,
                    }],
                    footer,
                    opt: FormatOptions {
                        color: SHOULD_COLORIZE.should_colorize(),
                        ..FormatOptions::default()
                    },
                };

                writeln!(writer, "{message}\n", message = DisplayList::from(snippet))?;
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
