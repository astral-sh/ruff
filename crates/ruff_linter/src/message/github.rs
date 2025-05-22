use std::io::Write;

use ruff_source_file::LineColumn;

use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Message};

/// Generate error workflow command in GitHub Actions format.
/// See: [GitHub documentation](https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions#setting-an-error-message)
#[derive(Default)]
pub struct GithubEmitter;

impl Emitter for GithubEmitter {
    fn emit(
        &mut self,
        writer: &mut dyn Write,
        messages: &[Message],
        context: &EmitterContext,
    ) -> anyhow::Result<()> {
        for message in messages {
            let source_location = message.compute_start_location();
            let location = if context.is_notebook(&message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                LineColumn::default()
            } else {
                source_location
            };

            let end_location = message.compute_end_location();

            write!(
                writer,
                "::error title=Ruff{code},file={file},line={row},col={column},endLine={end_row},endColumn={end_column}::",
                code = message
                    .to_noqa_code()
                    .map_or_else(String::new, |code| format!(" ({code})")),
                file = message.filename(),
                row = source_location.line,
                column = source_location.column,
                end_row = end_location.line,
                end_column = end_location.column,
            )?;

            write!(
                writer,
                "{path}:{row}:{column}:",
                path = relativize_path(&*message.filename()),
                row = location.line,
                column = location.column,
            )?;

            if let Some(code) = message.to_noqa_code() {
                write!(writer, " {code}")?;
            }

            writeln!(writer, " {}", message.body())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::GithubEmitter;
    use crate::message::tests::{
        capture_emitter_output, create_messages, create_syntax_error_messages,
    };

    #[test]
    fn output() {
        let mut emitter = GithubEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = GithubEmitter;
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());

        assert_snapshot!(content);
    }
}
