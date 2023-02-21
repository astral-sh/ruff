use log::error;

use rustpython_parser::ast::{Constant, ExprKind, Stmt, StmtKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::{
    ast::{types::Range, helpers::is_const_none}, checkers::ast::Checker, fix::Fix, registry::Diagnostic,
    violation::AlwaysAutofixableViolation, autofix::helpers::delete_stmt,
};

define_violation!(
    /// ## What it does
    /// Checks if the last statement of a function or a method is `return` or
    ///  `return None`.
    /// ## Why is this bad?
    /// The `__init__` method is the constructor for a given Python class,
    /// responsible for initializing, rather than creating, new objects.
    ///
    /// Python implicitly assumes `None` return value at the end of a function.
    /// This statement can be safely removed.
    ///
    /// ## Example
    /// ```python
    /// def some_fun():
    ///     print(5)
    ///     return None
    /// ```
    pub struct UselessReturn;
);

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
    let parent_statement: Option<&Stmt> =
        checker.current_stmt_parent().map(std::convert::Into::into);

    let belongs_to_function_scope = match parent_statement {
        Some(node) => matches!(node.node, StmtKind::FunctionDef { .. }),
        None => false,
    };
    let is_last_statement = checker.current_sibling_stmt() == None && belongs_to_function_scope;
    if !is_last_statement {
        return;
    }

    let is_bare_return_or_none = match value {
        None => true,
        Some(loc_expr) => is_const_none(loc_expr),
    };
    if is_bare_return_or_none {
        let mut diagnostic = Diagnostic::new(UselessReturn, Range::from_located(stmt));
        if checker.patch(diagnostic.kind.rule()) {
                // Fix::deletion(stmt.location, stmt.end_location.unwrap())
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
}
