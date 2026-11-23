use std::fmt::{self, Display, Write};

use ruff_text_size::TextRange;

use crate::diagnostic::{
    Annotation, Diagnostic, DisplayDiagnosticConfig, FileResolver, Severity, SubDiagnosticSeverity,
    UnifiedFile,
};

use super::Renderer;

pub struct GithubRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> GithubRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }
}

impl Renderer for GithubRenderer<'_> {
    fn render(&self, f: &mut fmt::Formatter, diagnostics: &[Diagnostic]) -> fmt::Result {
        for diagnostic in diagnostics {
            let severity = match diagnostic.severity() {
                Severity::Hint | Severity::Info => "notice",
                Severity::Warning => "warning",
                Severity::Error | Severity::Fatal => "error",
            };
            let use_name = self.config.preview && !self.config.prefer_rule_codes;
            let code = if use_name {
                diagnostic.id().as_str()
            } else {
                diagnostic.secondary_code_or_id()
            };
            write!(
                f,
                "::{severity} title={program} ({code})",
                program = escape_property(self.config.program),
                code = escape_property(code),
            )?;

            if let Some(span) = diagnostic.primary_span_ref() {
                let file = span.file();
                write!(f, ",file={}", escape_property(file.path(self.resolver)))?;

                let (start_location, end_location) = if self.resolver.is_notebook(file) {
                    // We can't give a reasonable location for the structured formats,
                    // so we show one that's clearly a fallback
                    None
                } else {
                    let diagnostic_source = file.diagnostic_source(self.resolver);
                    let source_code = diagnostic_source.as_source_code();

                    span.range().map(|range| {
                        (
                            source_code.line_column(range.start()),
                            source_code.line_column(range.end()),
                        )
                    })
                }
                .unwrap_or_default();

                // GitHub Actions workflow commands have constraints on error annotations:
                // - `col` and `endColumn` cannot be set if `line` and `endLine` are different
                // See: https://github.com/astral-sh/ruff/issues/22074
                if start_location.line == end_location.line {
                    write!(
                        f,
                        ",line={row},col={column},endLine={end_row},endColumn={end_column}::",
                        row = start_location.line,
                        column = start_location.column,
                        end_row = end_location.line,
                        end_column = end_location.column,
                    )?;
                } else {
                    write!(
                        f,
                        ",line={row},endLine={end_row}::",
                        row = start_location.line,
                        end_row = end_location.line,
                    )?;
                }

                write!(
                    f,
                    "{path}:{row}:{column}: ",
                    path = escape_message(file.relative_path(self.resolver).display()),
                    row = start_location.line,
                    column = start_location.column,
                )?;
            } else {
                write!(f, "::")?;
            }

            if !use_name && let Some(code) = diagnostic.secondary_code() {
                write!(f, "{}", escape_message(code))?;
            } else {
                write!(f, "{}:", escape_message(diagnostic.id()))?;
            }

            write!(f, " {}", escape_message(diagnostic.concise_message()))?;

            // After rendering the main diagnostic, render its secondary annotations and
            // sub-diagnostics. Note that lines within a single diagnostic must be separated by
            // URL-encoded newlines (`%0A`) to render properly in GitHub annotations.
            for annotation in diagnostic.secondary_annotations().filter_map(|annotation| {
                GithubAnnotation::from_annotation(annotation, self.resolver)
            }) {
                write!(f, "%0A{}", escape_message(annotation))?;
            }

            for subdiagnostic in diagnostic.sub_diagnostics() {
                let severity = match subdiagnostic.severity() {
                    SubDiagnosticSeverity::Help => "help",
                    SubDiagnosticSeverity::Info => "info",
                    SubDiagnosticSeverity::Warning => "warning",
                    SubDiagnosticSeverity::Error | SubDiagnosticSeverity::Fatal => "error",
                };
                let message = escape_message(subdiagnostic.concise_message());
                if let Some(annotation) = subdiagnostic.primary_annotation()
                    && let span = annotation.get_span()
                    && let file = span.file()
                    && let Some(range) = span.range()
                {
                    let diagnostic_source = file.diagnostic_source(self.resolver);
                    let source_code = diagnostic_source.as_source_code();
                    let start_location = source_code.line_column(range.start());
                    write!(
                        f,
                        "%0A  {path}:{row}:{column}: {severity}: {message}",
                        path = escape_message(file.relative_path(self.resolver).display()),
                        row = start_location.line,
                        column = start_location.column,
                    )?;
                } else {
                    write!(f, "%0A  {severity}: {message}")?;
                }

                for annotation in subdiagnostic
                    .secondary_annotations()
                    .filter_map(|annotation| {
                        GithubAnnotation::from_annotation(annotation, self.resolver)
                    })
                {
                    write!(f, "%0A  {}", escape_message(annotation))?;
                }
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

struct GithubAnnotation<'a> {
    message: &'a str,
    range: TextRange,
    file: &'a UnifiedFile,
    resolver: &'a dyn FileResolver,
}

impl<'a> GithubAnnotation<'a> {
    fn from_annotation(annotation: &'a Annotation, resolver: &'a dyn FileResolver) -> Option<Self> {
        let span = annotation.get_span();
        Some(Self {
            message: annotation.get_message()?,
            range: span.range()?,
            file: span.file(),
            resolver,
        })
    }
}

impl Display for GithubAnnotation<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let diagnostic_source = self.file.diagnostic_source(self.resolver);
        let source_code = diagnostic_source.as_source_code();
        let start_location = source_code.line_column(self.range.start());
        write!(
            f,
            "  {path}:{row}:{column}:",
            path = self.file.relative_path(self.resolver).display(),
            row = start_location.line,
            column = start_location.column,
        )?;

        write!(f, " {message}", message = self.message)
    }
}

/// Escape command data as in the [GitHub Actions toolkit].
///
/// [GitHub Actions toolkit]: https://github.com/actions/toolkit/blob/0c3a9c332b678ff1e776f93d728a4ad279803abe/packages/core/src/command.ts#L103-L117
fn escape_message(message: impl Display) -> impl Display {
    fmt::from_fn(move |f| write!(MessageWriter(f), "{message}"))
}

/// Property values must also escape the delimiters in the command header.
fn escape_property(text: &str) -> impl Display + '_ {
    fmt::from_fn(move |f| {
        let mut start = 0;
        for i in memchr::memchr2_iter(b':', b',', text.as_bytes()) {
            MessageWriter(f).write_str(&text[start..i])?;
            f.write_str(if text.as_bytes()[i] == b':' {
                "%3A"
            } else {
                "%2C"
            })?;
            start = i + 1;
        }
        MessageWriter(f).write_str(&text[start..])
    })
}

/// Escape each fragment of a formatted message without buffering it.
struct MessageWriter<'a, 'b>(&'a mut fmt::Formatter<'b>);

impl Write for MessageWriter<'_, '_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut start = 0;
        for i in memchr::memchr3_iter(b'%', b'\r', b'\n', text.as_bytes()) {
            self.0.write_str(&text[start..i])?;
            self.0.write_str(match text.as_bytes()[i] {
                b'%' => "%25",
                b'\r' => "%0D",
                _ => "%0A",
            })?;
            start = i + 1;
        }
        self.0.write_str(&text[start..])
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Severity;
    use crate::diagnostic::render::tests::{
        DiagnosticFormat, TestEnvironment, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn escapes_command_properties_and_messages() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Github);
        let path = "café,part:%0A.py";
        env.add(path, "x");
        let diag = env
            .builder("test", Severity::Error, "expected operator:\r\nbad\n^^^")
            .primary(path, "1:0", "1:1", "")
            .secondary(path, "1:0", "1:1", "first\nsecond")
            .help("try 100%, or %0A")
            .build();

        assert_eq!(
            env.render(&diag),
            concat!(
                "::error title=ty (test),file=/café%2Cpart%3A%250A.py,line=1,col=1,endLine=1,endColumn=2::",
                "café,part:%250A.py:1:1: test: expected operator:%0D%0Abad%0A^^^",
                "%0A  café,part:%250A.py:1:1: first%0Asecond",
                "%0A  help: try 100%25, or %250A\n",
            )
        );
    }

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Github);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Github);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Github);

        let diag = env.err().build();

        insta::assert_snapshot!(
            env.render(&diag),
            @"::error title=ty (test-diagnostic)::test-diagnostic: main diagnostic message",
        );
    }
}
