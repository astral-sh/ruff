use std::io::Write;

use ruff_db::diagnostic::Diagnostic;
use ruff_source_file::LineColumn;

use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext};

/// Generate error workflow command in GitHub Actions format.
/// See: [GitHub documentation](https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions#setting-an-error-message)
#[derive(Default)]
pub struct GithubEmitter;

impl Emitter for GithubEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        diagnostics: &[Diagnostic],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for diagnostic in diagnostics {
            let source_location = diagnostic.expect_ruff_start_location();
            let filename = diagnostic.expect_ruff_filename();
            let location = if context.is_notebook(&filename) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                LineColumn::default()
            } else {
                source_location
            };

            let end_location = diagnostic.expect_ruff_end_location();

            write!(
                writer,
                "::error title=Ruff ({code}),file={file},line={row},col={column},endLine={end_row},endColumn={end_column}::",
                code = diagnostic.secondary_code_or_id(),
                file = filename,
                row = source_location.line,
                column = source_location.column,
                end_row = end_location.line,
                end_column = end_location.column,
            )?;

            write!(
                writer,
                "{path}:{row}:{column}:",
                path = relativize_path(&filename),
                row = location.line,
                column = location.column,
            )?;

            if let Some(code) = diagnostic.secondary_code() {
                write!(writer, " {code}")?;
            } else {
                write!(writer, " {id}:", id = diagnostic.id())?;
            }

            writeln!(writer, " {}", diagnostic.body())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::GithubEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_diagnostics, create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let mut emitter = GithubEmitter;
        let content = capture_emitter_output(&mut emitter, &create_diagnostics());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = GithubEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_diagnostics());

        assert_snapshot!(content);
    }
}
