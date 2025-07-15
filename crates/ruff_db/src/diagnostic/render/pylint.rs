use std::path::Path;

use crate::diagnostic::{Diagnostic, SecondaryCode, render::FileResolver};

/// Generate violations in Pylint format.
///
/// The format is given by this string:
///
/// ```python
/// "%(path)s:%(row)d: [%(code)s] %(text)s"
/// ```
///
/// See: [Flake8 documentation](https://flake8.pycqa.org/en/latest/internal/formatters.html#pylint-formatter)
pub(super) struct PylintRenderer<'a> {
    resolver: &'a dyn FileResolver,
}

impl<'a> PylintRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver) -> Self {
        Self { resolver }
    }
}

impl PylintRenderer<'_> {
    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        for diagnostic in diagnostics {
            let mut filename = None;
            let mut row = None;
            if let Some(span) = diagnostic.primary_span_ref() {
                let file = span.file();
                filename = Some(file.path(self.resolver));
                if !self.resolver.is_notebook(file) {
                    if let Some(range) = span.range() {
                        row = Some(
                            file.diagnostic_source(self.resolver)
                                .as_source_code()
                                .line_column(range.start())
                                .line,
                        )
                    }
                }
            };

            let code = diagnostic
                .secondary_code()
                .map_or_else(|| diagnostic.name(), SecondaryCode::as_str);

            let filename = filename.unwrap_or_default();
            let row = row.unwrap_or_default();

            writeln!(
                f,
                "{path}:{row}: [{code}] {body}",
                path = relativize_path(filename),
                body = diagnostic.body()
            )?;
        }

        Ok(())
    }
}

/// Convert an absolute path to be relative to the current working directory.
fn relativize_path<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();

    let cwd = get_cwd();
    if let Ok(path) = path.strip_prefix(cwd) {
        return path.display().to_string();
    }

    path.display().to_string()
}

/// Return the current working directory.
///
/// On WASM this just returns `.`. Otherwise, defer to [`path_absolutize::path_dedot::CWD`].
fn get_cwd() -> &'static Path {
    #[cfg(target_arch = "wasm32")]
    {
        use std::path::PathBuf;
        static CWD: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| PathBuf::from("."));
        &CWD
    }
    #[cfg(not(target_arch = "wasm32"))]
    path_absolutize::path_dedot::CWD.as_path()
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{TestEnvironment, create_diagnostics, create_syntax_error_diagnostics},
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Pylint);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Pylint);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn missing_file() {
        let mut env = TestEnvironment::new();
        env.format(DiagnosticFormat::Pylint);

        let diag = env.err().build();

        insta::assert_snapshot!(
            env.render(&diag),
            @":1: [test-diagnostic] main diagnostic message",
        );
    }
}
