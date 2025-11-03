use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for explicit `raise StopIteration` in generator functions.
///
/// ## Why is this bad?
/// Raising `StopIteration` in a generator function causes a `RuntimeError`
/// when the generator is iterated over.
///
/// Instead of `raise StopIteration`, use `return` in generator functions.
///
/// ## Example
/// ```python
/// def my_generator():
///     yield 1
///     yield 2
///     raise StopIteration  # This causes RuntimeError at runtime
/// ```
///
/// Use instead:
/// ```python
/// def my_generator():
///     yield 1
///     yield 2
///     return  # Use return instead
/// ```
///
/// ## References
/// - [PEP 479](https://peps.python.org/pep-0479/)
/// - [Python documentation](https://docs.python.org/3/library/exceptions.html#StopIteration)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.3")]
pub(crate) struct StopIterationReturn;

impl Violation for StopIterationReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Explicit `raise StopIteration` in generator".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `return` instead".to_string())
    }
}

/// PLR1708
pub(crate) fn stop_iteration_return(checker: &Checker, raise_stmt: &ast::StmtRaise) {
    // Fast-path: only continue if this is `raise StopIteration` (with or without args)
    let Some(exc) = &raise_stmt.exc else {
        return;
    };

    let is_stop_iteration = match exc.as_ref() {
        ast::Expr::Call(ast::ExprCall { func, .. }) => {
            checker.semantic().match_builtin_expr(func, "StopIteration")
        }
        expr => checker.semantic().match_builtin_expr(expr, "StopIteration"),
    };

    if !is_stop_iteration {
        return;
    }

    // Now check the (more expensive) generator context
    if !in_generator_context(checker) {
        return;
    }

    checker.report_diagnostic(StopIterationReturn, raise_stmt.range());
}

/// Returns true if we're inside a function that contains any `yield`/`yield from`.
fn in_generator_context(checker: &Checker) -> bool {
    for scope in checker.semantic().current_scopes() {
        if let ruff_python_semantic::ScopeKind::Function(function_def) = scope.kind {
            if contains_yield_statement(&function_def.body) {
                return true;
            }
        }
    }
    false
}

/// Check if a statement list contains any yield statements
fn contains_yield_statement(body: &[ast::Stmt]) -> bool {
    struct YieldFinder {
        found: bool,
    }

    impl Visitor<'_> for YieldFinder {
        fn visit_expr(&mut self, expr: &ast::Expr) {
            if matches!(expr, ast::Expr::Yield(_) | ast::Expr::YieldFrom(_)) {
                self.found = true;
            } else {
                walk_expr(self, expr);
            }
        }
    }

    let mut finder = YieldFinder { found: false };
    for stmt in body {
        walk_stmt(&mut finder, stmt);
        if finder.found {
            return true;
        }
    }
    false
}
