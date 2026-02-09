use serde::{Serialize, Serializer, ser::SerializeSeq};
use serde_json::{Value, json};

use ruff_diagnostics::{Applicability, Edit};
use ruff_notebook::NotebookIndex;
use ruff_source_file::{LineColumn, OneIndexed};
use ruff_text_size::Ranged;

use crate::diagnostic::{ConciseMessage, Diagnostic, DiagnosticSource, DisplayDiagnosticConfig};

use super::FileResolver;

pub(super) struct JsonRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> JsonRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }
}

impl JsonRenderer<'_> {
    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        write!(
            f,
            "{:#}",
            diagnostics_to_json_value(diagnostics, self.resolver, self.config)
        )
    }
}

fn diagnostics_to_json_value<'a>(
    diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
    resolver: &dyn FileResolver,
    config: &DisplayDiagnosticConfig,
) -> Value {
    let values: Vec<_> = diagnostics
        .into_iter()
        .map(|diag| diagnostic_to_json(diag, resolver, config))
        .collect();
    json!(values)
}

pub(super) fn diagnostic_to_json<'a>(
    diagnostic: &'a Diagnostic,
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
) -> JsonDiagnostic<'a> {
    let span = diagnostic.primary_span_ref();
    let filename = span.map(|span| span.file().path(resolver));
    let range = span.and_then(|span| span.range());
    let diagnostic_source = span.map(|span| span.file().diagnostic_source(resolver));
    let source_code = diagnostic_source
        .as_ref()
        .map(|diagnostic_source| diagnostic_source.as_source_code());
    let notebook_index = span.and_then(|span| resolver.notebook_index(span.file()));

    let mut start_location = None;
    let mut end_location = None;
    let mut noqa_location = None;
    let mut notebook_cell_index = None;
    if let Some(source_code) = source_code {
        noqa_location = diagnostic
            .noqa_offset()
            .map(|offset| source_code.line_column(offset));
        if let Some(range) = range {
            let mut start = source_code.line_column(range.start());
            let mut end = source_code.line_column(range.end());
            if let Some(notebook_index) = &notebook_index {
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

    let fix = diagnostic.fix().map(|fix| JsonFix {
        applicability: fix.applicability(),
        message: diagnostic.first_help_text(),
        edits: ExpandedEdits {
            edits: fix.edits(),
            notebook_index,
            config,
            diagnostic_source,
        },
    });

    // In preview, the locations and filename can be optional.
    if config.preview {
        JsonDiagnostic {
            code: diagnostic.secondary_code_or_id(),
            url: diagnostic.documentation_url(),
            message: diagnostic.concise_message(),
            fix,
            cell: notebook_cell_index,
            location: start_location.map(JsonLocation::from),
            end_location: end_location.map(JsonLocation::from),
            filename,
            noqa_row: noqa_location.map(|location| location.line),
        }
    } else {
        JsonDiagnostic {
            code: diagnostic.secondary_code_or_id(),
            url: diagnostic.documentation_url(),
            message: diagnostic.concise_message(),
            fix,
            cell: notebook_cell_index,
            location: Some(start_location.unwrap_or_default().into()),
            end_location: Some(end_location.unwrap_or_default().into()),
            filename: Some(filename.unwrap_or_default()),
            noqa_row: noqa_location.map(|location| location.line),
        }
    }
}

struct ExpandedEdits<'a> {
    edits: &'a [Edit],
    notebook_index: Option<NotebookIndex>,
    config: &'a DisplayDiagnosticConfig,
    diagnostic_source: Option<DiagnosticSource>,
}

impl Serialize for ExpandedEdits<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.edits.len()))?;

        for edit in self.edits {
            let (location, end_location) = if let Some(diagnostic_source) = &self.diagnostic_source
            {
                let source_code = diagnostic_source.as_source_code();
                let mut location = source_code.line_column(edit.start());
                let mut end_location = source_code.line_column(edit.end());

                if let Some(notebook_index) = &self.notebook_index {
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

            // In preview, the locations can be optional.
            let value = if self.config.preview {
                JsonEdit {
                    content: edit.content().unwrap_or_default(),
                    location: location.map(JsonLocation::from),
                    end_location: end_location.map(JsonLocation::from),
                }
            } else {
                JsonEdit {
                    content: edit.content().unwrap_or_default(),
                    location: Some(location.unwrap_or_default().into()),
                    end_location: Some(end_location.unwrap_or_default().into()),
                }
            };

            s.serialize_element(&value)?;
        }

        s.end()
    }
}

/// A serializable version of `Diagnostic`.
///
/// The `Old` variant only exists to preserve backwards compatibility. Both this and `JsonEdit`
/// should become structs with the `New` definitions in a future Ruff release.
#[derive(Serialize)]
pub(crate) struct JsonDiagnostic<'a> {
    cell: Option<OneIndexed>,
    code: &'a str,
    end_location: Option<JsonLocation>,
    filename: Option<&'a str>,
    fix: Option<JsonFix<'a>>,
    location: Option<JsonLocation>,
    message: ConciseMessage<'a>,
    noqa_row: Option<OneIndexed>,
    url: Option<&'a str>,
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
    end_location: Option<JsonLocation>,
    location: Option<JsonLocation>,
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{
            TestEnvironment, create_diagnostics, create_notebook_diagnostics,
            create_syntax_error_diagnostics,
        },
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Json);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Json);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn notebook_output() {
        let (env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Json);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file_stable() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Json);
        env.preview(false);

        let diag = env
            .err()
            .documentation_url("https://docs.astral.sh/ruff/rules/test-diagnostic")
            .build();

        insta::assert_snapshot!(
            env.render(&diag),
            @r#"
        [
          {
            "cell": null,
            "code": "test-diagnostic",
            "end_location": {
              "column": 1,
              "row": 1
            },
            "filename": "",
            "fix": null,
            "location": {
              "column": 1,
              "row": 1
            },
            "message": "main diagnostic message",
            "noqa_row": null,
            "url": "https://docs.astral.sh/ruff/rules/test-diagnostic"
          }
        ]
        "#,
        );
    }

    #[test]
    fn missing_file_preview() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Json);
        env.preview(true);

        let diag = env
            .err()
            .documentation_url("https://docs.astral.sh/ruff/rules/test-diagnostic")
            .build();

        insta::assert_snapshot!(
            env.render(&diag),
            @r#"
        [
          {
            "cell": null,
            "code": "test-diagnostic",
            "end_location": null,
            "filename": null,
            "fix": null,
            "location": null,
            "message": "main diagnostic message",
            "noqa_row": null,
            "url": "https://docs.astral.sh/ruff/rules/test-diagnostic"
          }
        ]
        "#,
        );
    }
}
