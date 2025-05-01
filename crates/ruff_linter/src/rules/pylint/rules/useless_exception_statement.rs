use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::builtins;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for an exception that is not raised.
///
/// ## Why is this bad?
/// It's unnecessary to create an exception without raising it. For example,
/// `ValueError("...")` on its own will have no effect (unlike
/// `raise ValueError("...")`) and is likely a mistake.
///
/// ## Known problems
/// This rule only detects built-in exceptions, like `ValueError`, and does
/// not catch user-defined exceptions.
///
/// ## Example
/// ```python
/// ValueError("...")
/// ```
///
/// Use instead:
/// ```python
/// raise ValueError("...")
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as converting a useless exception
/// statement to a `raise` statement will change the program's behavior.
#[derive(ViolationMetadata)]
pub(crate) struct UselessExceptionStatement;

impl Violation for UselessExceptionStatement {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing `raise` statement on exception".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add `raise` keyword".to_string())
    }
}

/// PLW0133
pub(crate) fn useless_exception_statement(checker: &Checker, expr: &ast::StmtExpr) {
    let Expr::Call(ast::ExprCall { func, .. }) = expr.value.as_ref() else {
        return;
    };

    if is_builtin_exception(func, checker.semantic(), checker.target_version()) {
        let mut diagnostic = Diagnostic::new(UselessExceptionStatement, expr.range());
        diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
            "raise ".to_string(),
            expr.start(),
        )));
        checker.report_diagnostic(diagnostic);
    }
}

/// Returns `true` if the given expression is a builtin exception.
fn is_builtin_exception(
    expr: &Expr,
    semantic: &SemanticModel,
    target_version: PythonVersion,
) -> bool {
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["" | "builtins", name]
            if builtins::is_exception(name, target_version.minor))
        })
}
