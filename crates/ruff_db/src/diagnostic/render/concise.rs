use crate::diagnostic::{
    Diagnostic, DisplayDiagnosticConfig, Severity,
    stylesheet::{DiagnosticStylesheet, fmt_styled, fmt_with_hyperlink},
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

        let sep = fmt_styled(":", stylesheet.separator);
        for diag in diagnostics {
            if self.config.is_canceled() {
                return Ok(());
            }

            if let Some(span) = diag.primary_span() {
                write!(
                    f,
                    "{path}",
                    path = fmt_styled(
                        span.file().relative_path(self.resolver).to_string_lossy(),
                        stylesheet.emphasis
                    )
                )?;
                if let Some(range) = span.range() {
                    let diagnostic_source = span.file().diagnostic_source(self.resolver);
                    let start = diagnostic_source
                        .as_source_code()
                        .line_column(range.start());

                    if let Some(notebook_index) = self.resolver.notebook_index(span.file()) {
                        write!(
                            f,
                            "{sep}cell {cell}{sep}{line}{sep}{col}",
                            cell = notebook_index.cell(start.line).unwrap_or_default(),
                            line = notebook_index.cell_row(start.line).unwrap_or_default(),
                            col = start.column,
                        )?;
                    } else {
                        write!(
                            f,
                            "{sep}{line}{sep}{col}",
                            line = start.line,
                            col = start.column,
                        )?;
                    }
                }
                write!(f, "{sep} ")?;
            }

            if self.config.hide_severity {
                if let Some(code) = diag.secondary_code() {
                    write!(
                        f,
                        "{code} ",
                        code = fmt_styled(
                            fmt_with_hyperlink(&code, diag.documentation_url(), &stylesheet),
                            stylesheet.secondary_code
                        )
                    )?;
                } else {
                    write!(
                        f,
                        "{id}: ",
                        id = fmt_styled(
                            fmt_with_hyperlink(
                                &diag.inner.id,
                                diag.documentation_url(),
                                &stylesheet
                            ),
                            stylesheet.secondary_code
                        )
                    )?;
                }
                if self.config.show_fix_status {
                    // Do not display an indicator for inapplicable fixes
                    if diag.has_applicable_fix(self.config) {
                        write!(f, "[{fix}] ", fix = fmt_styled("*", stylesheet.separator))?;
                    }
                }
            } else {
                let (severity, severity_style) = match diag.severity() {
                    Severity::Info => ("info", stylesheet.info),
                    Severity::Warning => ("warning", stylesheet.warning),
                    Severity::Error => ("error", stylesheet.error),
                    Severity::Fatal => ("fatal", stylesheet.error),
                };
                write!(
                    f,
                    "{severity}[{id}] ",
                    severity = fmt_styled(severity, severity_style),
                    id = fmt_styled(
                        fmt_with_hyperlink(&diag.id(), diag.documentation_url(), &stylesheet),
                        stylesheet.emphasis
                    )
                )?;
            }

            writeln!(f, "{message}", message = diag.concise_message())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ruff_diagnostics::Applicability;

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
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        fib.py:1:8: error[unused-import] `os` imported but unused
        fib.py:6:5: error[unused-variable] Local variable `x` is assigned to but never used
        undef.py:1:4: error[undefined-name] Undefined name `a`
        ");
    }

    #[test]
    fn show_fixes() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Concise);
        env.hide_severity(true);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        fib.py:1:8: F401 [*] `os` imported but unused
        fib.py:6:5: F841 [*] Local variable `x` is assigned to but never used
        undef.py:1:4: F821 Undefined name `a`
        ");
    }

    #[test]
    fn show_fixes_preview() {
        let (mut env, diagnostics) = create_diagnostics(DiagnosticFormat::Concise);
        env.hide_severity(true);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);
        env.preview(true);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        fib.py:1:8: F401 [*] `os` imported but unused
        fib.py:6:5: F841 [*] Local variable `x` is assigned to but never used
        undef.py:1:4: F821 Undefined name `a`
        ");
    }

    #[test]
    fn show_fixes_syntax_errors() {
        let (mut env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Concise);
        env.hide_severity(true);
        env.show_fix_status(true);
        env.fix_applicability(Applicability::DisplayOnly);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        syntax_errors.py:1:15: invalid-syntax: Expected one or more symbol names after import
        syntax_errors.py:3:12: invalid-syntax: Expected ')', found newline
        ");
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Concise);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        syntax_errors.py:1:15: error[invalid-syntax] Expected one or more symbol names after import
        syntax_errors.py:3:12: error[invalid-syntax] Expected ')', found newline
        ");
    }

    #[test]
    fn notebook_output() {
        let (env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::Concise);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        notebook.ipynb:cell 1:2:8: error[unused-import] `os` imported but unused
        notebook.ipynb:cell 2:2:8: error[unused-import] `math` imported but unused
        notebook.ipynb:cell 3:4:5: error[unused-variable] Local variable `x` is assigned to but never used
        ");
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
