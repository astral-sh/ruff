use std::io::Write;

use ruff_source_file::SourceLocation;

use crate::fs::{relativize_path, relativize_path_to};
use crate::message::{Emitter, EmitterContext, Message};

/// Generate error workflow command in GitHub Actions format.
/// See: [GitHub documentation](https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions#setting-an-error-message)
pub struct GithubEmitter {
    project_dir: Option<String>,
}

impl Default for GithubEmitter {
    fn default() -> Self {
        Self {
            project_dir: std::env::var("GITHUB_WORKSPACE").ok(),
        }
    }
}

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
            let path = self.project_dir.as_ref().map_or_else(
                || relativize_path(message.filename()),
                |project_dir| relativize_path_to(message.filename(), project_dir),
            );

            write!(
                writer,
                "::error title=Ruff{code},file={file},line={row},col={column},endLine={end_row},endColumn={end_column}::",
                code = message.rule().map_or_else(String::new, |rule| format!(" ({})", rule.noqa_code())),
                file = path,
                row = source_location.row,
                column = source_location.column,
                end_row = end_location.row,
                end_column = end_location.column,
            )?;

            write!(
                writer,
                "{path}:{row}:{column}:",
                path = path,
                row = location.row,
                column = location.column,
            )?;

            if let Some(rule) = message.rule() {
                write!(writer, " {}", rule.noqa_code())?;
            }

            writeln!(writer, " {}", message.body())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use crate::message::tests::{
        capture_emitter_output, create_messages, create_syntax_error_messages,
    };
    use crate::message::GithubEmitter;

    #[test]
    fn output() {
        let mut emitter = GithubEmitter {
            project_dir: Some(String::from("/projects/project")),
        };
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn output_absolute_path() {
        let mut emitter = GithubEmitter { project_dir: None };
        let content = capture_emitter_output(&mut emitter, &create_messages());

        assert_snapshot!(content);
    }

    #[test]
    fn syntax_errors() {
        let mut emitter = GithubEmitter {
            project_dir: Some(String::from("/projects/project")),
        };
        let content = capture_emitter_output(&mut emitter, &create_syntax_error_messages());

        assert_snapshot!(content);
    }
}
