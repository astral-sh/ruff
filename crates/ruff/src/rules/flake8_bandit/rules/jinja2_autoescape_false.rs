use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct Jinja2AutoescapeFalse {
    pub value: bool,
}

impl Violation for Jinja2AutoescapeFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Jinja2AutoescapeFalse { value } = self;
        match value {
            true => format!(
                "Using jinja2 templates with `autoescape=False` is dangerous and can lead to XSS. \
                 Ensure `autoescape=True` or use the `select_autoescape` function."
            ),
            false => format!(
                "By default, jinja2 sets `autoescape` to `False`. Consider using \
                 `autoescape=True` or the `select_autoescape` function to mitigate XSS \
                 vulnerabilities."
            ),
        }
    }
}

/// S701
pub fn jinja2_autoescape_false(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["jinja2", "Environment"]
        })
    {
        let call_args = SimpleCallArgs::new(args, keywords);

        if let Some(autoescape_arg) = call_args.keyword_argument("autoescape") {
            match &autoescape_arg.node {
                ExprKind::Constant {
                    value: Constant::Bool(true),
                    ..
                } => (),
                ExprKind::Call { func, .. } => {
                    if let ExprKind::Name { id, .. } = &func.node {
                        if id.as_str() != "select_autoescape" {
                            checker.diagnostics.push(Diagnostic::new(
                                Jinja2AutoescapeFalse { value: true },
                                Range::from(autoescape_arg),
                            ));
                        }
                    }
                }
                _ => checker.diagnostics.push(Diagnostic::new(
                    Jinja2AutoescapeFalse { value: true },
                    Range::from(autoescape_arg),
                )),
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                Jinja2AutoescapeFalse { value: false },
                Range::from(func),
            ));
        }
    }
}
