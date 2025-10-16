use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use ruff_python_ast::visitor::{walk_stmt, walk_expr, Visitor};

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for explicit `raise StopIteration` in generator functions.
///
/// ## Why is this bad?
/// Raising `StopIteration` in a generator function causes a `RuntimeError`
/// at runtime (even in Python 3.7+, the earliest version we support). This
/// breaks the abstraction between generators and iterators and will crash
/// the program when the generator is iterated over.
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

/// PLR1708
pub(crate) fn stop_iteration_return(checker: &Checker, raise_stmt: &ast::StmtRaise) {
    // Check if we're in a generator function (function that contains yield statements)
    let mut in_generator_function = false;
    for scope in checker.semantic().current_scopes() {
        if let ruff_python_semantic::ScopeKind::Function(function_def) = scope.kind {
            // Check if this function contains any yield statements by traversing its body
            if contains_yield_statement(&function_def.body) {
                in_generator_function = true;
                break;
            }
        }
    }
    
    if !in_generator_function {
        return;
    }

    // Check if the raise statement is raising StopIteration
    let Some(exc) = &raise_stmt.exc else {
        return;
    };

    // Check if it's a StopIteration exception (could be with or without a value)
    if let ast::Expr::Call(ast::ExprCall {
        func,
        arguments: _,
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
            if id == "StopIteration" && checker.semantic().has_builtin_binding("StopIteration") {
                // It's a StopIteration being raised with arguments
                checker.report_diagnostic(StopIterationReturn, raise_stmt.range());
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
        if id == "StopIteration" && checker.semantic().has_builtin_binding("StopIteration") {
            checker.report_diagnostic(StopIterationReturn, raise_stmt.range());
        }
    }
}
