use ruff_python_ast::helpers::find_keyword;
use rustpython_parser::ast::{self, Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct Jinja2AutoescapeFalse {
    value: bool,
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
pub(crate) fn jinja2_autoescape_false(checker: &mut Checker, func: &Expr, keywords: &[Keyword]) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["jinja2", "Environment"])
        })
    {
        if let Some(keyword) = find_keyword(keywords, "autoescape") {
            match &keyword.value {
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Bool(true),
                    ..
                }) => (),
                Expr::Call(ast::ExprCall { func, .. }) => {
                    if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                        if id != "select_autoescape" {
                            checker.diagnostics.push(Diagnostic::new(
                                Jinja2AutoescapeFalse { value: true },
                                keyword.range(),
                            ));
                        }
                    }
                }
                _ => checker.diagnostics.push(Diagnostic::new(
                    Jinja2AutoescapeFalse { value: true },
                    keyword.range(),
                )),
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                Jinja2AutoescapeFalse { value: false },
                func.range(),
            ));
        }
    }
}
