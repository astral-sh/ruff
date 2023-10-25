use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers::{is_const_none, ReturnStatementVisitor};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
///
/// ## Why is this bad?
///
/// ## Examples
/// ```python
/// ```
///
/// Use instead:
/// ```python
/// ```
#[violation]
pub struct FunctionReturnHintNone;
// TODO naming convention

impl AlwaysFixableViolation for FunctionReturnHintNone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function should use `-> None` return type hint")
    }

    fn fix_title(&self) -> String {
        format!("Replace with `-> None`")
    }
}

/// Return `true` if a function's return statement include at least one
/// non-`None` value.
// FIXME copy-paste
pub(super) fn result_exists(returns: &[&ast::StmtReturn]) -> bool {
    returns.iter().any(|stmt| {
        stmt.value.as_deref().is_some_and(|value| {
            !matches!(
                value,
                Expr::Constant(constant) if constant.value.is_none()
            )
        })
    })
}

/// RUF300
pub(crate) fn function_return_hint_none(
    checker: &mut Checker,
    function_def: &ast::StmtFunctionDef,
    body: &[Stmt],
    returns: Option<&Expr>,
) {
    // Already hinted `-> None`, ignore
    if returns.map_or(false, is_const_none) {
        return;
    }

    // Find the last statement in the function.
    // let Some(last_stmt) = body.last() else {
    //     Skip empty functions.
    //     return;
    // };

    // Traverse the function body, to collect the stack.
    let returns = {
        let mut visitor = ReturnStatementVisitor::default();
        visitor.visit_body(body);
        visitor.returns
    };

    // TODO: Avoid false positives for generators.
    // if stack.is_generator {
    //     return;
    // }

    // If we have at least one non-`None` return...
    if result_exists(&returns) {
        return;
    }

    let edit = edit_function_return_type(function_def, "None".to_string());
    let mut diagnostic = Diagnostic::new(FunctionReturnHintNone, function_def.parameters.range);
    // Mark as unsafe if we're *changing* an existing return type
    diagnostic.set_fix(if function_def.returns.is_none() {
        Fix::safe_edit(edit)
    } else {
        Fix::unsafe_edit(edit)
    });
    checker.diagnostics.push(diagnostic);
}

fn edit_function_return_type(function_def: &ast::StmtFunctionDef, new_type: String) -> Edit {
    if let Some(returns) = &function_def.returns {
        Edit::range_replacement(new_type, returns.range())
    } else {
        Edit::insertion(
            format!(" -> {}", new_type),
            function_def.parameters.range.end(),
        )
    }
}
