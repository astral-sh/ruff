use std::io::Write;

use ruff_diagnostics::Applicability;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};

use ruff_db::diagnostic::{Diagnostic, SecondaryCode};
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed, SourceCode};
use ruff_text_size::Ranged;

use crate::Edit;
use crate::message::{Emitter, EmitterContext};

#[derive(Default)]
pub struct JsonEmitter;

impl Emitter for JsonEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(
            writer,
            &ExpandedMessages {
                diagnostics,
                context,
            },
        )?;

        Ok(())
    }
}

struct ExpandedMessages<'a> {
    diagnostics: &'a [Diagnostic],
    context: &'a EmitterContext<'a>,
}

impl Serialize for ExpandedMessages<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.diagnostics.len()))?;

        for message in self.diagnostics {
            let value = message_to_json_value(message, self.context);
            s.serialize_element(&value)?;
        }

        s.end()
    }
}

pub(crate) fn message_to_json_value<'a>(
    message: &'a Diagnostic,
    context: &'a EmitterContext<'a>,
) -> JsonDiagnostic<'a> {
    let source_file = message.expect_ruff_source_file();
    let source_code = source_file.to_source_code();
    let filename = message.expect_ruff_filename();
    let notebook_index = context.notebook_index(&filename);

    let mut start_location = source_code.line_column(message.expect_range().start());
    let mut end_location = source_code.line_column(message.expect_range().end());
    let mut noqa_location = message
        .noqa_offset()
        .map(|offset| source_code.line_column(offset));
    let mut notebook_cell_index = None;

    if let Some(notebook_index) = notebook_index {
        notebook_cell_index = Some(
            notebook_index
                .cell(start_location.line)
                .unwrap_or(OneIndexed::MIN),
        );
        start_location = notebook_index.translate_line_column(&start_location);
        end_location = notebook_index.translate_line_column(&end_location);
        noqa_location =
            noqa_location.map(|location| notebook_index.translate_line_column(&location));
    }

    let fix = message.fix().map(|fix| JsonFix {
        applicability: fix.applicability(),
        message: message.suggestion(),
        edits: ExpandedEdits {
            edits: fix.edits(),
            source_code,
            notebook_index,
        },
    });

    JsonDiagnostic {
        code: message.secondary_code(),
        url: message.to_url(),
        message: message.body(),
        fix,
        cell: notebook_cell_index,
        location: start_location.into(),
        end_location: end_location.into(),
        filename,
        noqa_row: noqa_location.map(|location| location.line),
    }
}

struct ExpandedEdits<'a> {
    edits: &'a [Edit],
    source_code: SourceCode<'a, 'a>,
    notebook_index: Option<&'a NotebookIndex>,
}

impl Serialize for ExpandedEdits<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.edits.len()))?;

        for edit in self.edits {
            let mut location = self.source_code.line_column(edit.start());
            let mut end_location = self.source_code.line_column(edit.end());

            if let Some(notebook_index) = self.notebook_index {
                // There exists a newline between each cell's source code in the
                // concatenated source code in Ruff. This newline doesn't actually
                // exists in the JSON source field.
                //
                // Now, certain edits may try to remove this newline, which means
                // the edit will spill over to the first character of the next cell.
                // If it does, we need to translate the end location to the last
                // character of the previous cell.
                match (
                    notebook_index.cell(location.line),
                    notebook_index.cell(end_location.line),
                ) {
                    (Some(start_cell), Some(end_cell)) if start_cell != end_cell => {
                        debug_assert_eq!(end_location.column.get(), 1);

                        let prev_row = end_location.line.saturating_sub(1);
                        end_location = LineColumn {
                            line: notebook_index.cell_row(prev_row).unwrap_or(OneIndexed::MIN),
                            column: self
                                .source_code
                                .line_column(self.source_code.line_end_exclusive(prev_row))
                                .column,
                        };
                    }
                    (Some(_), None) => {
                        debug_assert_eq!(end_location.column.get(), 1);

                        let prev_row = end_location.line.saturating_sub(1);
                        end_location = LineColumn {
                            line: notebook_index.cell_row(prev_row).unwrap_or(OneIndexed::MIN),
                            column: self
                                .source_code
                                .line_column(self.source_code.line_end_exclusive(prev_row))
                                .column,
                        };
                    }
                    _ => {
                        end_location = notebook_index.translate_line_column(&end_location);
                    }
                }
                location = notebook_index.translate_line_column(&location);
            }

            let value = JsonEdit {
                content: edit.content().unwrap_or_default(),
                location: location.into(),
                end_location: end_location.into(),
            };

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

#[derive(Serialize)]
pub(crate) struct JsonDiagnostic<'a> {
    cell: Option<OneIndexed>,
    code: Option<&'a SecondaryCode>,
    end_location: JsonLocation,
    filename: String,
    fix: Option<JsonFix<'a>>,
    location: JsonLocation,
    message: &'a str,
    noqa_row: Option<OneIndexed>,
    url: Option<String>,
}

#[derive(Serialize)]
struct JsonFix<'a> {
    applicability: Applicability,
    edits: ExpandedEdits<'a>,
    message: Option<&'a str>,
}

#[derive(Serialize)]
struct JsonLocation {
    column: OneIndexed,
    row: OneIndexed,
}

impl From<LineColumn> for JsonLocation {
    fn from(location: LineColumn) -> Self {
        JsonLocation {
            row: location.line,
            column: location.column,
        }
    }
}

#[derive(Serialize)]
struct JsonEdit<'a> {
    content: &'a str,
    end_location: JsonLocation,
    location: JsonLocation,
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::JsonEmitter;
    use crate::message::tests::{
        capture_emitter_notebook_output, capture_emitter_output, create_diagnostics,
        create_notebook_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = JsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = JsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn notebook_output() {
        let mut emitter = JsonEmitter;
        let (diagnostics, notebook_indexes) = create_notebook_diagnostics();
        let content =
            capture_emitter_notebook_output(&mut emitter, &diagnostics, &notebook_indexes);

        assert_snapshot!(content);
    }
}
