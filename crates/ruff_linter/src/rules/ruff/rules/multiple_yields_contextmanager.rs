use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, visitor::source_order};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::function_def_visit_preorder_except_body;

/// ## What it does
/// Checks that a function decorated with `contextlib.contextmanager` yields only once.
///
/// ### Why is this bad?
/// When using `contextlib.contextmanager` all code preceding the   `yield` is setup code and all
/// code after the `yield` is cleanup code that runs when exiting the context.
///
/// A second `yield` in the cleanup phase breaks the context manager protocol and results in a
/// runtime error.
///
/// ## Example
/// ```python
/// @contextlib.contextmanager
/// def broken_context_manager():
///     print("Setting up")
///     yield "first value"  # This yield is expected
///     print("Some cleanup")
///     yield "second value"  # This violates the protocol
/// ```
///
/// Use instead:
/// ```python
/// @contextlib.contextmanager
/// def correct_context_manager():
///     print("Setting up")
///     yield "value"  # Single yield
///     print("Cleanup code runs when exiting the context")
/// ```
///
/// ## References
/// - [Python documentation: contextlib.contextmanager](https://docs.python.org/3/library/contextlib.html#contextlib.contextmanager)
/// - [Python documentation: contextlib.asynccontextmanager](https://docs.python.org/3/library/contextlib.html#contextlib.asynccontextmanager)
#[derive(ViolationMetadata)]
pub(crate) struct MultipleYieldsInContextManager;

impl Violation for MultipleYieldsInContextManager {
    const FIX_AVAILABILITY: ruff_diagnostics::FixAvailability =
        ruff_diagnostics::FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Function decorated with `contextlib.contextmanager` may yield more than once".to_string()
    }
}

/// RUF060
pub(crate) fn multiple_yields_in_contextmanager(
    checker: &Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !is_contextmanager_decorated(function_def, checker) {
        return;
    }
    let yield_count = count_yields_in_fn(function_def);
    if yield_count > 1 {
        checker.report_diagnostic(Diagnostic::new(
            MultipleYieldsInContextManager,
            function_def.identifier(),
        ));
    }
}

fn is_contextmanager_decorated(function_def: &ast::StmtFunctionDef, checker: &Checker) -> bool {
    for decorator in &function_def.decorator_list {
        if let Some(qualified) = checker
            .semantic()
            .resolve_qualified_name(map_callable(&decorator.expression))
        {
            if matches!(
                qualified.segments(),
                ["contextlib", "contextmanager" | "asynccontextmanager"]
            ) {
                return true;
            }
        }
    }
    false
}

#[derive(Default)]
struct YieldCounter {
    count: usize,
}

impl<'a> source_order::SourceOrderVisitor<'a> for YieldCounter {
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                self.count += 1;
            }
            _ => source_order::walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(nested_fn) => {
                function_def_visit_preorder_except_body(nested_fn, self);
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }
}

fn count_yields_in_fn(fn_def: &ast::StmtFunctionDef) -> usize {
    let mut counter = YieldCounter::default();
    source_order::walk_body(&mut counter, &fn_def.body);
    counter.count
}
