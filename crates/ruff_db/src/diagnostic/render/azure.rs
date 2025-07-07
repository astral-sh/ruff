use ruff_source_file::LineColumn;

use crate::diagnostic::Severity;

use super::DisplayDiagnostic;

impl DisplayDiagnostic<'_> {
    pub(super) fn azure(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let severity = match self.diag.severity() {
            Severity::Info | Severity::Warning => "warning",
            Severity::Error | Severity::Fatal => "error",
        };
        write!(f, "##vso[task.logissue type={severity};")?;
        if let Some(span) = self.diag.primary_span() {
            let filename = span.file().path(self.resolver);
            write!(f, "sourcepath={filename};")?;
            if let Some(range) = span.range() {
                let location = if self.resolver.notebook_index(span.file()).is_some() {
                    // We can't give a reasonable location for the structured formats,
                    // so we show one that's clearly a fallback
                    LineColumn::default()
                } else {
                    span.file()
                        .diagnostic_source(self.resolver)
                        .as_source_code()
                        .line_column(range.start())
                };
                write!(
                    f,
                    "linenumber={line};columnnumber={col};",
                    line = location.line,
                    col = location.column,
                )?;
            }
        }
        writeln!(
            f,
            "{code}]{body}",
            code = self
                .diag
                .secondary_code()
                .map_or_else(String::new, |code| format!("code={code};")),
            body = self.diag.body(),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{DiagnosticFormat, render::tests::create_diagnostics};

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Azure);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }
}
