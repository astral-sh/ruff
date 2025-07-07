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
        .into_iter()
        .map(|diag| message_to_json_value(diag, resolver))
        .collect();
    json!(messages)
}

pub(super) fn message_to_json_value(message: &Diagnostic, resolver: &dyn FileResolver) -> Value {
    let span = message.primary_span_ref();
    let filename = span.map(|span| span.file().path(resolver));
    let range = span.and_then(|span| span.range());
    let diagnostic_source = span.map(|span| span.file().diagnostic_source(resolver));
    let source_code = diagnostic_source
        .as_ref()
        .map(|diagnostic_source| diagnostic_source.as_source_code());
    let notebook_index = span.and_then(|span| resolver.notebook_index(span.file()));

    let fix = message.fix().map(|fix| {
        json!({
            "applicability": fix.applicability(),
            "message": message.suggestion(),
            "edits": &ExpandedEdits {
                edits: fix.edits(),
                source_code: source_code.as_ref(),
                notebook_index: notebook_index.as_ref()
            },
        })
    });

    let mut start_location = None;
    let mut end_location = None;
    let mut noqa_location = None;
    let mut notebook_cell_index = None;
    if let Some(source_code) = source_code {
        noqa_location = message
            .noqa_offset()
            .map(|offset| source_code.line_column(offset));
        if let Some(range) = range {
            let mut start = source_code.line_column(range.start());
            let mut end = source_code.line_column(range.end());
            if let Some(notebook_index) = notebook_index {
                notebook_cell_index =
                    Some(notebook_index.cell(start.line).unwrap_or(OneIndexed::MIN));
                start = notebook_index.translate_line_column(&start);
                end = notebook_index.translate_line_column(&end);
                noqa_location =
                    noqa_location.map(|location| notebook_index.translate_line_column(&location));
            }
            start_location = Some(start);
            end_location = Some(end);
        }
    }

    json!({
        "code": message.secondary_code(),
        "url": message.to_url(),
        "message": message.body(),
        "fix": fix,
        "cell": notebook_cell_index,
        "location": start_location.map(location_to_json),
        "end_location": end_location.map(location_to_json),
        "filename": filename,
        "noqa_row": noqa_location.map(|location| location.line)
    })
}

fn location_to_json(location: LineColumn) -> serde_json::Value {
    json!({
        "row": location.line,
        "column": location.column
    })
}

struct ExpandedEdits<'a> {
    edits: &'a [Edit],
    source_code: Option<&'a SourceCode<'a, 'a>>,
    notebook_index: Option<&'a NotebookIndex>,
}

impl Serialize for ExpandedEdits<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.edits.len()))?;

        for edit in self.edits {
            let (location, end_location) = if let Some(source_code) = self.source_code {
                let mut location = source_code.line_column(edit.start());
                let mut end_location = source_code.line_column(edit.end());

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
                                column: source_code
                                    .line_column(source_code.line_end_exclusive(prev_row))
                                    .column,
                            };
                        }
                        (Some(_), None) => {
                            debug_assert_eq!(end_location.column.get(), 1);

                            let prev_row = end_location.line.saturating_sub(1);
                            end_location = LineColumn {
                                line: notebook_index.cell_row(prev_row).unwrap_or(OneIndexed::MIN),
                                column: source_code
                                    .line_column(source_code.line_end_exclusive(prev_row))
                                    .column,
                            };
                        }
                        _ => {
                            end_location = notebook_index.translate_line_column(&end_location);
                        }
                    }
                    location = notebook_index.translate_line_column(&location);
                }

                (Some(location), Some(end_location))
            } else {
                (None, None)
            };

            let value = json!({
                "content": edit.content().unwrap_or_default(),
                "location": location.map(location_to_json),
                "end_location": end_location.map(location_to_json)
            });

            s.serialize_element(&value)?;
        }

        s.end()
    }
}
