use std::io::Write;

use ruff_source_file::SourceLocation;

use crate::fs::relativize_path;
use crate::message::{Emitter, EmitterContext, Message};
use crate::registry::AsRule;

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
            let location = if context.is_notebook(message.filename()) {
                // We can't give a reasonable location for the structured formats,
                // so we show one that's clearly a fallback
                SourceLocation::default()
            } else {
                source_location.clone()
            };

            let end_location = message.compute_end_location();

            write!(
                writer,
                "::error title=Ruff \
                         ({code}),file={file},line={row},col={column},endLine={end_row},endColumn={end_column}::",
                code = message.kind.rule().noqa_code(),
                file = message.filename(),
                row = source_location.row,
                column = source_location.column,
                end_row = end_location.row,
                end_column = end_location.column,
            )?;

            writeln!(
                writer,
                "{path}:{row}:{column}: {code} {body}",
                path = relativize_path(message.filename()),
                row = location.row,
                column = location.column,
                code = message.kind.rule().noqa_code(),
                body = message.kind.body,
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{capture_emitter_output, create_messages};
    use crate::message::GithubEmitter;

    #[test]
    fn output() {
        let mut emitter = GithubEmitter;
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }
}
