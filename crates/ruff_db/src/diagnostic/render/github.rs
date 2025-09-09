use crate::diagnostic::{Diagnostic, FileResolver};

pub(super) struct GithubRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> GithubRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        for diagnostic in diagnostics {
            write!(
                f,
                "::error title=Ruff ({code})",
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

                write!(
                    f,
                    ",line={row},col={column},endLine={end_row},endColumn={end_column}::",
                    row = start_location.line,
                    column = start_location.column,
                    end_row = end_location.line,
                    end_column = end_location.column,
                )?;

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

            writeln!(f, " {}", diagnostic.body())?;
        }

        Ok(())
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
            @"::error title=Ruff (test-diagnostic)::test-diagnostic: main diagnostic message",
        );
    }
}
