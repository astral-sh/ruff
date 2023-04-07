use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;
use std::io::Write;

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
            let (row, column) = if context.is_jupyter_notebook(message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                (1, 0)
            } else {
                (message.location.row(), message.location.column())
            };

            write!(
                writer,
                "::error title=Ruff \
                         ({code}),file={file},line={row},col={column},endLine={end_row},endColumn={end_column}::",
                code = message.kind.rule().noqa_code(),
                file = message.filename(),
                row = message.location.row(),
                column = message.location.column(),
                end_row = message.end_location.row(),
                end_column = message.end_location.column(),
            )?;

            writeln!(
                writer,
                "{path}:{row}:{column}: {code} {body}",
                path = relativize_path(message.filename()),
                code = message.kind.rule().noqa_code(),
                body = message.kind.body,
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::GithubEmitter;
    use insta::assert_snapshot;

    #[test]
    fn output() {
        let mut emitter = GithubEmitter::default();
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
