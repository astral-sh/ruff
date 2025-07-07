#[cfg(test)]
mod tests {
    use crate::diagnostic::{
        DiagnosticFormat,
        render::tests::{
            create_diagnostics, create_notebook_diagnostics, create_syntax_error_diagnostics,
        },
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
