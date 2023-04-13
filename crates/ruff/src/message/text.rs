use crate::fs::relativize_path;
use crate::message::diff::Diff;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use annotate_snippets::display_list::{DisplayList, FormatOptions};
use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};
use colored::Colorize;
use ruff_diagnostics::DiagnosticKind;
use ruff_python_ast::source_code::OneIndexed;
use ruff_text_size::TextRange;
use std::cmp;
use std::fmt::{Display, Formatter};
use std::io::Write;

#[derive(Default)]
pub struct TextEmitter {
    show_fix_status: bool,
    show_fix: bool,
}

impl TextEmitter {
    #[must_use]
    pub fn with_show_fix_status(mut self, show_fix_status: bool) -> Self {
        self.show_fix_status = show_fix_status;
        self
    }

    #[must_use]
    pub fn with_show_fix(mut self, show_fix: bool) -> Self {
        self.show_fix = show_fix;
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

            // Check if we're working on a jupyter notebook and translate positions with cell accordingly
            let (row, col) = if let Some(jupyter_index) = context.jupyter_index(message.filename())
            {
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

            if message.file.source_code().is_some() {
                writeln!(writer, "{}", MessageCodeFrame { message })?;

                if self.show_fix {
                    if let Some(diff) = Diff::from_message(message) {
                        writeln!(writer, "{diff}")?;
                    }
                }
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
            file,
            location,
            end_location,
            ..
        } = self.message;

        if let Some(source_code) = file.source_code() {
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

            let mut start_index =
                OneIndexed::new(cmp::max(1, location.row().saturating_sub(2))).unwrap();
            let content_start_index = OneIndexed::new(location.row()).unwrap();

            // Trim leading empty lines.
            while start_index < content_start_index {
                if !source_code.line_text(start_index).trim().is_empty() {
                    break;
                }
                start_index = start_index.saturating_add(1);
            }

            let mut end_index = OneIndexed::new(cmp::min(
                end_location.row().saturating_add(2),
                source_code.line_count() + 1,
            ))
            .unwrap();

            let content_end_index = OneIndexed::new(end_location.row()).unwrap();

            // Trim trailing empty lines
            while end_index > content_end_index {
                if !source_code.line_text(end_index).trim().is_empty() {
                    break;
                }

                end_index = end_index.saturating_sub(1);
            }

            let start_offset = source_code.line_start(start_index);
            let end_offset = source_code.line_end(end_index);

            let source_text = &source_code.text()[TextRange::new(start_offset, end_offset)];

            let annotation_start_offset =
                // Message columns are one indexed
                source_code.offset(location.with_col_offset(-1)) - start_offset;
            let annotation_end_offset =
                source_code.offset(end_location.with_col_offset(-1)) - start_offset;

            let start_char = source_text[TextRange::up_to(annotation_start_offset)]
                .chars()
                .count();

            let char_length = source_text
                [TextRange::new(annotation_start_offset, annotation_end_offset)]
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
                        range: (start_char, start_char + char_length),
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
