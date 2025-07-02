use std::io::Write;

use ruff_source_file::LineColumn;

use crate::message::{Emitter, EmitterContext, OldDiagnostic};

/// Generate error logging commands for Azure Pipelines format.
/// See [documentation](https://learn.microsoft.com/en-us/azure/devops/pipelines/scripts/logging-commands?view=azure-devops&tabs=bash#logissue-log-an-error-or-warning)
#[derive(Default)]
pub struct AzureEmitter;

impl Emitter for AzureEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[OldDiagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for diagnostic in diagnostics {
            let location = if context.is_notebook(&diagnostic.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                LineColumn::default()
            } else {
                diagnostic.compute_start_location()
            };

            writeln!(
                writer,
                "##vso[task.logissue type=error\
                        ;sourcepath={filename};linenumber={line};columnnumber={col};{code}]{body}",
                filename = diagnostic.filename(),
                line = location.line,
                col = location.column,
                code = diagnostic
                    .secondary_code()
                    .map_or_else(String::new, |code| format!("code={code};")),
                body = diagnostic.body(),
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::AzureEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = AzureEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = AzureEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }
}
