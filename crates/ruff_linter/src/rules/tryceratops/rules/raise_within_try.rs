use ruff_python_ast::{self as ast, ExceptHandler, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    comparable::ComparableExpr,
    helpers::{self, map_callable},
    statement_visitor::{walk_stmt, StatementVisitor},
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `raise` statements within `try` blocks. The only `raise`s
/// caught are those that throw exceptions caught by the `try` statement itself.
///
/// ## Why is this bad?
/// Raising and catching exceptions within the same `try` block is redundant,
/// as the code can be refactored to avoid the `try` block entirely.
///
/// Alternatively, the `raise` can be moved within an inner function, making
/// the exception reusable across multiple call sites.
///
/// ## Example
/// ```python
/// def bar():
///     pass
///
///
/// def foo():
///     try:
///         a = bar()
///         if not a:
///             raise ValueError
///     except ValueError:
///         raise
/// ```
///
/// Use instead:
/// ```python
/// def bar():
///     raise ValueError
///
///
/// def foo():
///     try:
///         a = bar()  # refactored `bar` to raise `ValueError`
///     except ValueError:
///         raise
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct RaiseWithinTry;

impl Violation for RaiseWithinTry {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Abstract `raise` to an inner function".to_string()
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<&'a Stmt>,
}

impl<'a> StatementVisitor<'a> for RaiseStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Raise(_) => self.raises.push(stmt),
            Stmt::Try(_) => (),
            _ => walk_stmt(self, stmt),
        }
    }
}

/// TRY301
pub(crate) fn raise_within_try(checker: &Checker, body: &[Stmt], handlers: &[ExceptHandler]) {
    if handlers.is_empty() {
        return;
    }

    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor.visit_body(body);
        visitor.raises
    };

    if raises.is_empty() {
        return;
    }

    let handled_exceptions = helpers::extract_handled_exceptions(handlers);
    let comparables: Vec<ComparableExpr> = handled_exceptions
        .iter()
        .map(|handler| ComparableExpr::from(*handler))
        .collect();

    for stmt in raises {
        let Stmt::Raise(ast::StmtRaise {
            exc: Some(exception),
            ..
        }) = stmt
        else {
            continue;
        };

        // We can't check exception sub-classes without a type-checker implementation, so let's
        // just catch the blanket `Exception` for now.
        if comparables.contains(&ComparableExpr::from(map_callable(exception)))
            || handled_exceptions.iter().any(|expr| {
                checker
                    .semantic()
                    .resolve_builtin_symbol(expr)
                    .is_some_and(|builtin| matches!(builtin, "Exception" | "BaseException"))
            })
        {
            checker.report_diagnostic(Diagnostic::new(RaiseWithinTry, stmt.range()));
        }
    }
}
