use serde::{Serialize, Serializer, ser::SerializeSeq};
use serde_json::{Value, json};

use ruff_diagnostics::Edit;
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed, SourceCode};
use ruff_text_size::Ranged;

use crate::diagnostic::Diagnostic;

use super::FileResolver;

pub(super) fn diagnostics_to_json_value<'a>(
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
    resolver: &dyn FileResolver,
) -> Value {
    let messages: Vec<_> = diagnostics
        .iter()
        .filter_map(|diag| message_to_json_value(diag, resolver))
        .collect();
    json!(messages)
}

pub(super) fn message_to_json_value(
    message: &Diagnostic,
    resolver: &dyn FileResolver,
) -> Option<Value> {
    let span = message.primary_span()?;
    let filename = span.file().path(resolver);
    let range = span.range()?;
    let diagnostic_source = span.file().diagnostic_source(resolver);
    let source_code = diagnostic_source.as_source_code();
    // Input can be a notebook for ty, but we don't have a good way of retrieving the notebook
    // index for Ruff. we might just need to pass it in
    let notebook_index = None; // TODO

    let fix = message.fix().map(|fix| {
        json!({
            "applicability": fix.applicability(),
            "message": message.suggestion(),
            "edits": &ExpandedEdits { edits: fix.edits(), source_code: &source_code, notebook_index },
        })
    });

    let mut start_location = source_code.line_column(range.start());
    let mut end_location = source_code.line_column(range.end());
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

    Some(json!({
        "code": message.secondary_code(),
        "url": message.to_url(),
        "message": message.body(),
        "fix": fix,
        "cell": notebook_cell_index,
        "location": location_to_json(start_location),
        "end_location": location_to_json(end_location),
        "filename": filename,
        "noqa_row": noqa_location.map(|location| location.line)
    }))
}

fn location_to_json(location: LineColumn) -> serde_json::Value {
    json!({
        "row": location.line,
        "column": location.column
    })
}

struct ExpandedEdits<'a> {
    edits: &'a [Edit],
    source_code: &'a SourceCode<'a, 'a>,
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

            let value = json!({
                "content": edit.content().unwrap_or_default(),
                "location": location_to_json(location),
                "end_location": location_to_json(end_location)
            });

            s.serialize_element(&value)?;
        }

        s.end()
    }
}
