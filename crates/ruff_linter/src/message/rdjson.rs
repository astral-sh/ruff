use std::io::Write;

use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};

use ruff_db::diagnostic::{Diagnostic, SecondaryCode};
use ruff_diagnostics::Fix;
use ruff_source_file::{OneIndexed, SourceCode};
use ruff_text_size::Ranged;

use crate::Edit;
use crate::message::{Emitter, EmitterContext, LineColumn};

#[derive(Default)]
pub struct RdjsonEmitter;

impl Emitter for RdjsonEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        _context: &EmitterContext,
    ) -> anyhow::Result<()> {
        serde_json::to_writer_pretty(writer, &RdjsonDiagnostics::new(diagnostics))?;

        Ok(())
    }
}

struct ExpandedDiagnostics<'a> {
    diagnostics: &'a [Diagnostic],
}

impl Serialize for ExpandedDiagnostics<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.diagnostics.len()))?;

        for diagnostic in self.diagnostics {
            let value = RdjsonDiagnostic::from(diagnostic);
            s.serialize_element(&value)?;
        }

        s.end()
    }
}

impl<'a> From<&'a Diagnostic> for RdjsonDiagnostic<'a> {
    fn from(diagnostic: &Diagnostic) -> RdjsonDiagnostic {
        let source_file = diagnostic.expect_ruff_source_file();
        let source_code = source_file.to_source_code();

        let start_location = source_code.line_column(diagnostic.expect_range().start());
        let end_location = source_code.line_column(diagnostic.expect_range().end());

        let edits = diagnostic.fix().map(Fix::edits).unwrap_or_default();

        RdjsonDiagnostic {
            message: diagnostic.body(),
            location: RdjsonLocation {
                path: diagnostic.expect_ruff_filename(),
                range: RdjsonRange::new(start_location, end_location),
            },
            code: RdjsonCode {
                value: diagnostic.secondary_code(),
                url: diagnostic.to_ruff_url(),
            },
            suggestions: rdjson_suggestions(edits, &source_code),
        }
    }
}

fn rdjson_suggestions<'a>(
    edits: &'a [Edit],
    source_code: &SourceCode,
) -> Vec<RdjsonSuggestion<'a>> {
    edits
        .iter()
        .map(|edit| {
            let location = source_code.line_column(edit.start());
            let end_location = source_code.line_column(edit.end());

            RdjsonSuggestion {
                range: RdjsonRange::new(location, end_location),
                text: edit.content().unwrap_or_default(),
            }
        })
        .collect()
}

#[derive(Serialize)]
struct RdjsonDiagnostics<'a> {
    diagnostics: ExpandedDiagnostics<'a>,
    severity: &'static str,
    source: RdjsonSource,
}

impl<'a> RdjsonDiagnostics<'a> {
    fn new(diagnostics: &'a [Diagnostic]) -> Self {
        Self {
            source: RdjsonSource {
                name: "ruff",
                url: env!("CARGO_PKG_HOMEPAGE"),
            },
            severity: "warning",
            diagnostics: ExpandedDiagnostics { diagnostics },
        }
    }
}

#[derive(Serialize)]
struct RdjsonSource {
    name: &'static str,
    url: &'static str,
}

#[derive(Serialize)]
struct RdjsonDiagnostic<'a> {
    code: RdjsonCode<'a>,
    location: RdjsonLocation,
    message: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    suggestions: Vec<RdjsonSuggestion<'a>>,
}

#[derive(Serialize)]
struct RdjsonLocation {
    path: String,
    range: RdjsonRange,
}

#[derive(Serialize)]
struct RdjsonRange {
    end: RdjsonLineColumn,
    start: RdjsonLineColumn,
}

impl RdjsonRange {
    fn new(start: LineColumn, end: LineColumn) -> Self {
        Self {
            start: start.into(),
            end: end.into(),
        }
    }
}

// This is an exact copy of `LineColumn` with the field order reversed to match the serialization
// behavior of `json!`.
#[derive(Serialize)]
struct RdjsonLineColumn {
    column: OneIndexed,
    line: OneIndexed,
}

impl From<LineColumn> for RdjsonLineColumn {
    fn from(value: LineColumn) -> Self {
        let LineColumn { line, column } = value;
        Self { column, line }
    }
}

#[derive(Serialize)]
struct RdjsonCode<'a> {
    url: Option<String>,
    value: Option<&'a SecondaryCode>,
}

#[derive(Serialize)]
struct RdjsonSuggestion<'a> {
    range: RdjsonRange,
    text: &'a str,
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::RdjsonEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = RdjsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = RdjsonEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }
}
