use std::fmt::{self, Display, Write};

use ruff_source_file::LineColumn;

use crate::diagnostic::{Diagnostic, DisplayDiagnosticConfig, Severity};

use super::{FileResolver, Renderer};

/// Print diagnostics in the [Azure Pipelines] format.
///
/// [Azure Pipelines]: https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops&tabs=bash#logissue-log-an-error-or-warning
pub struct AzureRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> AzureRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }
}

impl Renderer for AzureRenderer<'_> {
    fn render(&self, f: &mut fmt::Formatter, diagnostics: &[Diagnostic]) -> fmt::Result {
        for diag in diagnostics {
            let severity = match diag.severity() {
                Severity::Hint | Severity::Info | Severity::Warning => "warning",
                Severity::Error | Severity::Fatal => "error",
            };
            write!(f, "##vso[task.logissue type={severity};")?;
            if let Some(span) = diag.primary_span_ref() {
                let filename = span.file().path(self.resolver);
                write!(f, "sourcepath={};", escape_property(filename))?;
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
            let code = if self.config.preview && !self.config.prefer_rule_codes {
                diag.id().as_str()
            } else {
                diag.secondary_code_or_id()
            };
            writeln!(
                f,
                "code={};]{}",
                escape_property(code),
                escape_message(diag.concise_message()),
            )?;
        }

        Ok(())
    }
}

/// Escape command data as in the [Azure Pipelines task library].
///
/// [Azure Pipelines task library]: https://github.com/microsoft/azure-pipelines-task-lib/blob/43673c958dc5fb40b898c5c7b5b7b5bf0ebd14bd/node/taskcommand.ts#L93-L110
fn escape_message(message: impl Display) -> impl Display {
    fmt::from_fn(move |f| write!(MessageWriter(f), "{message}"))
}

/// Property values must also escape the delimiters in the command header.
fn escape_property(text: &str) -> impl Display + '_ {
    fmt::from_fn(move |f| {
        let mut start = 0;
        for i in memchr::memchr2_iter(b';', b']', text.as_bytes()) {
            MessageWriter(f).write_str(&text[start..i])?;
            f.write_str(if text.as_bytes()[i] == b';' {
                "%3B"
            } else {
                "%5D"
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
                b'%' => "%AZP25",
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
        env.format(DiagnosticFormat::Azure);
        let path = "café;part]%0A.py";
        env.add(path, "x");
        let diag = env
            .builder(
                "test",
                Severity::Error,
                "expected operator:\r\nbad;]%0A\n^^^^^^^^",
            )
            .primary(path, "1:0", "1:1", "")
            .build();

        assert_eq!(
            env.render(&diag),
            concat!(
                "##vso[task.logissue type=error;sourcepath=/café%3Bpart%5D%AZP250A.py;linenumber=1;columnnumber=1;code=test;]",
                "expected operator:%0D%0Abad;]%AZP250A%0A^^^^^^^^\n",
            )
        );
    }

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Azure);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Azure);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }
}
