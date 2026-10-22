use crate::diagnostic::{Diagnostic, DisplayDiagnosticConfig, render::json::diagnostic_to_json};

use super::{FileResolver, Renderer};

/// Print diagnostics in JSON format, one per line.
///
/// This prints each diagnostic as a separate JSON object on its own line. See
/// [`JsonRenderer`](super::JsonRenderer) for an array of all diagnostics.
/// See <https://jsonlines.org/> for more details.
pub struct JsonLinesRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> JsonLinesRenderer<'a> {
    pub fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }
}

impl Renderer for JsonLinesRenderer<'_> {
    fn render(&self, f: &mut std::fmt::Formatter, diagnostics: &[Diagnostic]) -> std::fmt::Result {
        for diag in diagnostics {
            writeln!(
                f,
                "{}",
                serde_json::json!(diagnostic_to_json(diag, self.resolver, self.config))
            )?;
        }

        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use crate::diagnostic::render::tests::{
        DiagnosticFormat, create_diagnostics, create_notebook_diagnostics,
        create_syntax_error_diagnostics,
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::JsonLines);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::JsonLines);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }

    #[test]
    fn notebook_output() {
        let (env, diagnostics) = create_notebook_diagnostics(DiagnosticFormat::JsonLines);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics));
    }
}
