use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};

use ruff_diagnostics::{Edit, Fix};
use ruff_source_file::{LineColumn, SourceCode};
use ruff_text_size::Ranged;

use crate::diagnostic::Diagnostic;

use super::FileResolver;

pub struct RdjsonRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> RdjsonRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        write!(
            f,
            "{:#}",
            serde_json::json!(RdjsonDiagnostics::new(diagnostics, self.resolver))
        )
    }
}

struct ExpandedDiagnostics<'a> {
    resolver: &'a dyn FileResolver,
    diagnostics: &'a [Diagnostic],
}

impl Serialize for ExpandedDiagnostics<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.diagnostics.len()))?;

        for diagnostic in self.diagnostics {
            let value = diagnostic_to_rdjson(diagnostic, self.resolver);
            s.serialize_element(&value)?;
        }

        s.end()
    }
}

fn diagnostic_to_rdjson<'a>(
    diagnostic: &'a Diagnostic,
    resolver: &'a dyn FileResolver,
) -> RdjsonDiagnostic<'a> {
    let span = diagnostic.primary_span_ref();
    let diagnostic_source = span.map(|span| span.file().diagnostic_source(resolver));
    let source_code = diagnostic_source
        .as_ref()
        .map(|diagnostic_source| diagnostic_source.as_source_code());

    let mut range = None;
    if let Some(source_code) = &source_code {
        if let Some(diagnostic_range) = diagnostic.range() {
            let start = source_code.line_column(diagnostic_range.start());
            let end = source_code.line_column(diagnostic_range.end());
            range = Some(RdjsonRange::new(start, end));
        }
    }

    let edits = diagnostic.fix().map(Fix::edits).unwrap_or_default();

    let location = span
        .map(|span| span.file().path(resolver))
        .map(|path| RdjsonLocation { path, range });

    RdjsonDiagnostic {
        message: diagnostic.body(),
        location,
        code: RdjsonCode {
            value: diagnostic
                .secondary_code()
                .map_or_else(|| diagnostic.name(), |code| code.as_str()),
            url: diagnostic.to_ruff_url(),
        },
        suggestions: rdjson_suggestions(edits, source_code),
    }
}

fn rdjson_suggestions<'a>(
    edits: &'a [Edit],
    source_code: Option<SourceCode>,
) -> Vec<RdjsonSuggestion<'a>> {
    if edits.is_empty() {
        return Vec::new();
    }

    let Some(source_code) = source_code else {
        debug_assert!(false, "Expected a source file for a diagnostic with a fix");
        return Vec::new();
    };

    edits
        .iter()
        .map(|edit| {
            let start = source_code.line_column(edit.start());
            let end = source_code.line_column(edit.end());
            let range = RdjsonRange::new(start, end);

            RdjsonSuggestion {
                range,
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
    fn new(diagnostics: &'a [Diagnostic], resolver: &'a dyn FileResolver) -> Self {
        Self {
            source: RdjsonSource {
                name: "ruff",
                url: env!("CARGO_PKG_HOMEPAGE"),
            },
            severity: "WARNING",
            diagnostics: ExpandedDiagnostics {
                diagnostics,
                resolver,
            },
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
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<RdjsonLocation<'a>>,
    message: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    suggestions: Vec<RdjsonSuggestion<'a>>,
}

#[derive(Serialize)]
struct RdjsonLocation<'a> {
    path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    range: Option<RdjsonRange>,
}

#[derive(Default, Serialize)]
struct RdjsonRange {
    end: LineColumn,
    start: LineColumn,
}

impl RdjsonRange {
    fn new(start: LineColumn, end: LineColumn) -> Self {
        Self { start, end }
    }
}

#[derive(Serialize)]
struct RdjsonCode<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    value: &'a str,
}

#[derive(Serialize)]
struct RdjsonSuggestion<'a> {
    range: RdjsonRange,
    text: &'a str,
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{TestEnvironment, create_diagnostics, create_syntax_error_diagnostics},
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Rdjson);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Rdjson);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file_stable() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Rdjson);
        env.preview(false);

        let diag = env.err().build();

        insta::assert_snapshot!(env.render(&diag));
    }

    #[test]
    fn missing_file_preview() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Rdjson);
        env.preview(true);

        let diag = env.err().build();

        insta::assert_snapshot!(env.render(&diag));
    }
}
