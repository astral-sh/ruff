use crate::diagnostic::{
    Diagnostic, DisplayDiagnosticConfig, Severity,
    stylesheet::{DiagnosticStylesheet, fmt_styled},
};

use super::FileResolver;

pub(super) struct ConciseRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> ConciseRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        let stylesheet = if self.config.color {
            DiagnosticStylesheet::styled()
        } else {
            DiagnosticStylesheet::plain()
        };

        for diag in diagnostics {
            let (severity, severity_style) = match diag.severity() {
                Severity::Info => ("info", stylesheet.info),
                Severity::Warning => ("warning", stylesheet.warning),
                Severity::Error => ("error", stylesheet.error),
                Severity::Fatal => ("fatal", stylesheet.error),
            };
            write!(
                f,
                "{severity}[{id}]",
                severity = fmt_styled(severity, severity_style),
                id = fmt_styled(diag.id(), stylesheet.emphasis)
            )?;
            if let Some(span) = diag.primary_span() {
                write!(
                    f,
                    " {path}",
                    path = fmt_styled(span.file().path(self.resolver), stylesheet.emphasis)
                )?;
                if let Some(range) = span.range() {
                    let diagnostic_source = span.file().diagnostic_source(self.resolver);
                    let start = diagnostic_source
                        .as_source_code()
                        .line_column(range.start());

                    if let Some(notebook_index) = self.resolver.notebook_index(span.file()) {
                        write!(
                            f,
                            ":cell {cell}:{line}:{col}",
                            cell = fmt_styled(
                                notebook_index.cell(start.line).unwrap_or_default(),
                                stylesheet.emphasis
                            ),
                            line = fmt_styled(
                                notebook_index.cell_row(start.line).unwrap_or_default(),
                                stylesheet.emphasis
                            ),
                            col = fmt_styled(start.column, stylesheet.emphasis),
                        )?;
                    } else {
                        write!(
                            f,
                            ":{line}:{col}",
                            line = fmt_styled(start.line, stylesheet.emphasis),
                            col = fmt_styled(start.column, stylesheet.emphasis),
                        )?;
                    }
                }
                write!(f, ":")?;
            }
            writeln!(f, " {message}", message = diag.concise_message())?;
        }

        Ok(())
    }
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
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Concise);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Concise);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn notebook_output() {
        let (env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Concise);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Concise);

        let diag = env.err().build();

        insta::assert_snapshot!(
            env.render(&diag),
            @"error[test-diagnostic] main diagnostic message",
        );
    }
}
