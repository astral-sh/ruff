use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for explicit `raise StopIteration` in generator functions.
///
/// ## Why is this bad?
/// Raising `StopIteration` with a value in a generator function was deprecated
/// in Python 3.5 and removed in later versions. This pattern breaks the
/// abstraction between generators and iterators and can cause unexpected
/// behavior.
///
/// Instead of `raise StopIteration(value)`, use `return value` in generator
/// functions.
///
/// ## Example
/// ```python
/// def my_generator():
///     yield 1
///     yield 2
///     raise StopIteration("finished")  # This is problematic
/// ```
///
/// Use instead:
/// ```python
/// def my_generator():
///     yield 1
///     yield 2
///     return "finished"  # Use return instead
/// ```
///
/// ## References
/// - [PEP 479](https://peps.python.org/pep-0479/)
/// - [Python documentation](https://docs.python.org/3/library/exceptions.html#StopIteration)
#[derive(ViolationMetadata)]
pub(crate) struct StopIterationReturn;

impl Violation for StopIterationReturn {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Explicit `raise StopIteration` in generator; use `return` instead".to_string()
    }
}

/// PLR1708
pub(crate) fn stop_iteration_return(checker: &Checker, raise_stmt: &ast::StmtRaise) {
    // Check if we're in a generator function
    if !checker.semantic().current_scope().kind.is_generator() {
        return;
    }

    // Check if the raise statement is raising StopIteration
    let Some(exc) = &raise_stmt.exc else {
        return;
    };

    // Check if it's a StopIteration exception (could be with or without a value)
    if let ast::Expr::Call(ast::ExprCall {
        func,
        arguments,
        range: _,
        node_index: _,
    }) = exc.as_ref()
    {
        // Check if it's calling StopIteration
        if let ast::Expr::Name(ast::ExprName {
            id,
            ctx: _,
            range: _,
            node_index: _,
        }) = func.as_ref()
        {
            if id == "StopIteration" {
                // It's a StopIteration being raised with arguments
                checker.report_diagnostic(StopIterationReturn, raise_stmt.range());
                return;
            }
        }
    } else if let ast::Expr::Name(ast::ExprName {
        id,
        ctx: _,
        range: _,
        node_index: _,
    }) = exc.as_ref()
    {
        // Check if it's just `raise StopIteration` (without arguments)
        if id == "StopIteration" {
            checker.report_diagnostic(StopIterationReturn, raise_stmt.range());
            return;
        }
    }
}
