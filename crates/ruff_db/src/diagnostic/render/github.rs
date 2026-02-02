use crate::diagnostic::{Diagnostic, FileResolver, Severity};

pub struct GithubRenderer<'a> {
    resolver: &'a dyn FileResolver,
    program: &'a str,
}

impl<'a> GithubRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, program: &'a str) -> Self {
        Self { resolver, program }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        for diagnostic in diagnostics {
            let severity = match diagnostic.severity() {
                Severity::Info => "notice",
                Severity::Warning => "warning",
                Severity::Error | Severity::Fatal => "error",
            };
            write!(
                f,
                "::{severity} title={program} ({code})",
                program = self.program,
                code = diagnostic.secondary_code_or_id()
            )?;

            if let Some(span) = diagnostic.primary_span() {
                let file = span.file();
                write!(f, ",file={file}", file = file.path(self.resolver))?;

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
                    path = file.relative_path(self.resolver).display(),
                    row = start_location.line,
                    column = start_location.column,
                )?;
            } else {
                write!(f, "::")?;
            }

            if let Some(code) = diagnostic.secondary_code() {
                write!(f, "{code}")?;
            } else {
                write!(f, "{id}:", id = diagnostic.id())?;
            }

            writeln!(f, " {}", diagnostic.concise_message())?;
        }

        Ok(())
    }
}

pub struct DisplayGithubDiagnostics<'a> {
    renderer: &'a GithubRenderer<'a>,
    diagnostics: &'a [Diagnostic],
}

impl<'a> DisplayGithubDiagnostics<'a> {
    pub fn new(renderer: &'a GithubRenderer<'a>, diagnostics: &'a [Diagnostic]) -> Self {
        Self {
            renderer,
            diagnostics,
        }
    }
}

impl std::fmt::Display for DisplayGithubDiagnostics<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.renderer.render(f, self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{TestEnvironment, create_diagnostics, create_syntax_error_diagnostics},
    };

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
