use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast,
    helpers::map_callable,
    visitor::{Visitor, walk_expr, walk_stmt},
};
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
pub(crate) fn stop_iteration_return(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    let mut analyzer = GeneratorAnalyzer {
        checker,
        has_yield: false,
        stop_iteration_raises: Vec::new(),
    };

    analyzer.visit_body(&function_def.body);

    if analyzer.has_yield {
        for raise_stmt in analyzer.stop_iteration_raises {
            checker.report_diagnostic(StopIterationReturn, raise_stmt.range());
        }
    }
}

struct GeneratorAnalyzer<'a, 'b> {
    checker: &'a Checker<'b>,
    has_yield: bool,
    stop_iteration_raises: Vec<&'a ast::StmtRaise>,
}

impl<'a> Visitor<'a> for GeneratorAnalyzer<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(_) => {}
            ast::Stmt::Raise(raise @ ast::StmtRaise { exc: Some(exc), .. }) => {
                if self
                    .checker
                    .semantic()
                    .match_builtin_expr(map_callable(exc), "StopIteration")
                {
                    self.stop_iteration_raises.push(raise);
                }
                walk_stmt(self, stmt);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Lambda(_) => {}
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                self.has_yield = true;
                walk_expr(self, expr);
            }
            _ => walk_expr(self, expr),
        }
    }
}
