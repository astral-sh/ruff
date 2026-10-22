use crate::diagnostic::{Diagnostic, DisplayDiagnosticConfig, render::json::diagnostic_to_json};

use super::{FileResolver, Renderer};

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
