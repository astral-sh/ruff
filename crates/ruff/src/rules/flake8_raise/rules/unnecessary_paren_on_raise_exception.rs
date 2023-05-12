use rustpython_parser::ast::{self, Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::match_parens;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UnnecessaryParenOnRaiseException;

impl AlwaysAutofixableViolation for UnnecessaryParenOnRaiseException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses on raised exception")
    }

    fn autofix_title(&self) -> String {
        format!("Remove unnecessary parentheses")
    }
}

/// RSE102
pub(crate) fn unnecessary_paren_on_raise_exception(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Call(ast::ExprCall {
        func,
        args,
        keywords,
    }) = &expr.node
    {
        if args.is_empty() && keywords.is_empty() {
            let range = match_parens(func.end(), checker.locator)
                .expect("Expected call to include parentheses");
            let mut diagnostic = Diagnostic::new(UnnecessaryParenOnRaiseException, range);
            if checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::deletion(func.end(), range.end())));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
