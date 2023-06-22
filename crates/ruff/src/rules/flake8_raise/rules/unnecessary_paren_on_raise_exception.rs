use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::match_parens;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary parentheses on raised exceptions.
///
/// ## Why is this bad?
/// If an exception is raised without any arguments, parentheses are not
/// required, as the `raise` statement accepts either an exception instance
/// or an exception class (which is then implicitly instantiated).
///
/// Removing the parentheses makes the code more concise.
///
/// ## Example
/// ```python
/// raise TypeError()
/// ```
///
/// Use instead:
/// ```python
/// raise TypeError
/// ```
///
/// ## References
/// - [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
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
    if let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        range: _,
    }) = expr
    {
        if args.is_empty() && keywords.is_empty() {
            let range = match_parens(func.end(), checker.locator)
                .expect("Expected call to include parentheses");
            let mut diagnostic = Diagnostic::new(UnnecessaryParenOnRaiseException, range);
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::deletion(func.end(), range.end())));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
