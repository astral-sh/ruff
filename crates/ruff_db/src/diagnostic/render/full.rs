use ruff_annotate_snippets::Renderer as AnnotateRenderer;

use crate::diagnostic::render::{FileResolver, Resolved};
use crate::diagnostic::{Diagnostic, DisplayDiagnosticConfig, stylesheet::DiagnosticStylesheet};

pub(super) struct FullRenderer<'a> {
    resolver: &'a dyn FileResolver,
    config: &'a DisplayDiagnosticConfig,
}

impl<'a> FullRenderer<'a> {
    pub(super) fn new(resolver: &'a dyn FileResolver, config: &'a DisplayDiagnosticConfig) -> Self {
        Self { resolver, config }
    }

    pub(super) fn render(
        &self,
        f: &mut std::fmt::Formatter,
        diagnostics: &[Diagnostic],
    ) -> std::fmt::Result {
        let stylesheet = if self.config.color {
            DiagnosticStylesheet::styled()
        } else {
            DiagnosticStylesheet::plain()
        };

        let mut renderer = if self.config.color {
            AnnotateRenderer::styled()
        } else {
            AnnotateRenderer::plain()
        };

        renderer = renderer
            .error(stylesheet.error)
            .warning(stylesheet.warning)
            .info(stylesheet.info)
            .note(stylesheet.note)
            .help(stylesheet.help)
            .line_no(stylesheet.line_no)
            .emphasis(stylesheet.emphasis)
            .none(stylesheet.none);

        for diag in diagnostics {
            let resolved = Resolved::new(self.resolver, diag);
            let renderable = resolved.to_renderable(self.config.context);
            for diag in renderable.diagnostics.iter() {
                writeln!(f, "{}", renderer.render(diag.to_annotate()))?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}
