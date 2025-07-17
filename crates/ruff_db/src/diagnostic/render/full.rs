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
            let resolved = Resolved::new(self.resolver, diag, self.config);
            let renderable = resolved.to_renderable(self.config.context);
            for diag in renderable.diagnostics.iter() {
                writeln!(f, "{}", renderer.render(diag.to_annotate()))?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{create_diagnostics, create_syntax_error_diagnostics},
    };

    #[test]
    fn output() {
        let (env, diagnostics) = create_diagnostics(DiagnosticFormat::Full);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r#"
        error[unused-import]: `os` imported but unused
         --> fib.py:1:8
          |
        1 | import os
          |        ^^
          |
        help: Remove unused import: `os`

        error[unused-variable]: Local variable `x` is assigned to but never used
         --> fib.py:6:5
          |
        4 | def fibonacci(n):
        5 |     """Compute the nth number in the Fibonacci sequence."""
        6 |     x = 1
          |     ^
        7 |     if n == 0:
        8 |         return 0
          |
        help: Remove assignment to unused variable `x`

        error[undefined-name]: Undefined name `a`
         --> undef.py:1:4
          |
        1 | if a == 1: pass
          |    ^
          |
        "#);
    }

    #[test]
    fn syntax_errors() {
        let (env, diagnostics) = create_syntax_error_diagnostics(DiagnosticFormat::Full);
        insta::assert_snapshot!(env.render_diagnostics(&diagnostics), @r"
        error[invalid-syntax]: SyntaxError: Expected one or more symbol names after import
         --> syntax_errors.py:1:15
          |
        1 | from os import
          |               ^
        2 |
        3 | if call(foo
          |

        error[invalid-syntax]: SyntaxError: Expected ')', found newline
         --> syntax_errors.py:3:12
          |
        1 | from os import
        2 |
        3 | if call(foo
          |            ^
        4 |     def bar():
        5 |         pass
          |
        ");
    }
}
