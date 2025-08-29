use std::io::Write;

use ruff_db::diagnostic::{Diagnostic, FileResolver};

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
        GithubRenderer::new(context).render(writer, diagnostics)
    }
}

pub(super) struct GithubRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> GithubRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }

    pub(super) fn render(
        &self,
        f: &mut dyn Write,
        diagnostics: &[Diagnostic],
    ) -> anyhow::Result<()> {
        for diagnostic in diagnostics {
            write!(
                f,
                "::error title=Ruff ({code})",
                code = diagnostic.secondary_code_or_id()
            )?;

            if let Some(span) = diagnostic.primary_span() {
                let file = span.file();
                write!(f, ",file={file}", file = file.path(self.resolver))?;

                let (start_location, end_location) = if self.resolver.is_notebook(file) {
                    // We can't give a reasonable location for the structured formats,
                    // so we show one that's clearly a fallback
                    None
                } else {
                    let source_code = diagnostic.ruff_source_file().unwrap().to_source_code();

                    span.range().map(|range| {
                        (
                            source_code.line_column(range.start()),
                            source_code.line_column(range.end()),
                        )
                    })
                }
                .unwrap_or_default();

                write!(
                    f,
                    ",line={row},col={column},endLine={end_row},endColumn={end_column}::",
                    row = start_location.line,
                    column = start_location.column,
                    end_row = end_location.line,
                    end_column = end_location.column,
                )?;

                write!(
                    f,
                    "{path}:{row}:{column}:",
                    path = file.relative_path(self.resolver).display(),
                    row = start_location.line,
                    column = start_location.column,
                )?;
            }

            if let Some(code) = diagnostic.secondary_code() {
                write!(f, " {code}")?;
            } else {
                write!(f, " {id}:", id = diagnostic.id())?;
            }

            writeln!(f, " {}", diagnostic.body())?;
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
