use crate::diagnostic::{Diagnostic, SecondaryCode, render::FileResolver};

/// Generate violations in Pylint format.
///
/// The format is given by this string:
///
/// ```python
/// "%(path)s:%(row)d: [%(code)s] %(text)s"
/// ```
///
/// See: [Flake8 documentation](https://flake8.pycqa.org/en/latest/internal/formatters.html#pylint-formatter)
pub(super) struct PylintRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> PylintRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }
}

impl PylintRenderer<'_> {
    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        for diagnostic in diagnostics {
            let (filename, row) = diagnostic
                .primary_span_ref()
                .map(|span| {
                    let file = span.file();

                    let row = span
                        .range()
                        .filter(|_| !self.resolver.is_notebook(file))
                        .map(|range| {
                            file.diagnostic_source(self.resolver)
                                .as_source_code()
                                .line_column(range.start())
                                .line
                        });

                    (file.relative_path(self.resolver).to_string_lossy(), row)
                })
                .unwrap_or_default();

            let code = diagnostic
                .secondary_code()
                .map_or_else(|| diagnostic.name(), SecondaryCode::as_str);

            let row = row.unwrap_or_default();

            writeln!(
                f,
                "{path}:{row}: [{code}] {body}",
                path = filename,
                body = diagnostic.concise_message()
            )?;
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
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Pylint);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Pylint);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Pylint);

        let diag = env.err().build();

        insta::assert_snapshot!(
            env.render(&diag),
            @":1: [test-diagnostic] main diagnostic message",
        );
    }
}
