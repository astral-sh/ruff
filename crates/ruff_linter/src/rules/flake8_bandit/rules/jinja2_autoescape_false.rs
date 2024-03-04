use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `jinja2` templates that use `autoescape=False`.
///
/// ## Why is this bad?
/// `jinja2` templates that use `autoescape=False` are vulnerable to cross-site
/// scripting (XSS) attacks that allow attackers to execute arbitrary
/// JavaScript.
///
/// By default, `jinja2` sets `autoescape` to `False`, so it is important to
/// set `autoescape=True` or use the `select_autoescape` function to mitigate
/// XSS vulnerabilities.
///
/// ## Example
/// ```python
/// import jinja2
///
/// jinja2.Environment(loader=jinja2.FileSystemLoader("."))
/// ```
///
/// Use instead:
/// ```python
/// import jinja2
///
/// jinja2.Environment(loader=jinja2.FileSystemLoader("."), autoescape=True)
/// ```
///
/// ## References
/// - [Jinja documentation: API](https://jinja.palletsprojects.com/en/latest/api/#autoescaping)
/// - [Common Weakness Enumeration: CWE-94](https://cwe.mitre.org/data/definitions/94.html)
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
pub(crate) fn jinja2_autoescape_false(checker: &mut Checker, call: &ast::ExprCall) {
    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualifieed_name| {
            matches!(qualifieed_name.segments(), ["jinja2", "Environment"])
        })
    {
        if let Some(keyword) = call.arguments.find_keyword("autoescape") {
            match &keyword.value {
                Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, .. }) => (),
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
                call.func.range(),
            ));
        }
    }
}
