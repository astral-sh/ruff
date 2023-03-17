use log::error;
use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::types::Range;

use crate::autofix::helpers::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks if the last statement of a function or a method is `return` or
///  `return None`.
/// ## Why is this bad?
///
/// Python implicitly assumes `None` return value at the  end of a function.
/// This statement can be safely removed.
///
/// ## Example
/// ```python
/// def some_fun():
///     print(5)
///     return None
/// ```
#[violation]
pub struct UselessReturn;

impl AlwaysAutofixableViolation for UselessReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Useless return at end of function or method")
    }

    fn autofix_title(&self) -> String {
        format!("Remove useless return statement at the end of the function")
    }
}

pub fn useless_return(checker: &mut Checker, stmt: &Stmt) {
    let StmtKind::Return { value} = &stmt.node else {
        return;
    };
    let parent_statement: Option<&Stmt> = checker.ctx.current_stmt_parent().map(Into::into);

    let belongs_to_function_scope = match parent_statement {
        Some(node) => matches!(node.node, StmtKind::FunctionDef { .. }),
        None => false,
    };
    let is_last_function_statement =
        checker.ctx.current_sibling_stmt().is_none() && belongs_to_function_scope;
    if !is_last_function_statement {
        return;
    }

    let is_bare_return_or_none = match value {
        None => true,
        Some(loc_expr) => is_const_none(loc_expr),
    };
    if !is_bare_return_or_none {
        return;
    }
    let mut diagnostic = Diagnostic::new(UselessReturn, Range::from(stmt));
    if checker.patch(diagnostic.kind.rule()) {
        match delete_stmt(
            stmt,
            None,
            &[],
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => {
                error!("Failed to delete `return` statement: {}", e);
            }
        };
    }
    checker.diagnostics.push(diagnostic);
}
