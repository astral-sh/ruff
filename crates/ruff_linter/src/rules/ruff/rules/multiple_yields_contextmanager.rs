use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{self as ast, visitor::source_order};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::function_def_visit_preorder_except_body;

/// ## What it does
/// Checks that a function decorated with `contextlib.contextmanager` yields only once.
///
/// ### Why is this bad?
/// A context manager must yield exactly once. Multiple yields cause a runtime error.
///
/// ## Example
/// ```python
/// @contextlib.contextmanager
/// def broken_context_manager():
///     print("Setting up")
///     yield "first value"  # This yield is expected
///     print("Cleanup")
///     yield "second value"  # This violates the protocol
/// ```
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
    let mut path_tracker = YieldPathTracker::default();
    source_order::walk_body(&mut path_tracker, &function_def.body);

    if path_tracker.has_multiple_yields {
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

struct YieldPathTracker {
    has_multiple_yields: bool,
    path_yields: Vec<usize>,
    branch_stack: Vec<Vec<usize>>,
}

impl Default for YieldPathTracker {
    fn default() -> Self {
        Self {
            has_multiple_yields: false,
            path_yields: vec![0],
            branch_stack: vec![],
        }
    }
}

impl<'a> source_order::SourceOrderVisitor<'a> for YieldPathTracker {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> source_order::TraversalSignal {
        if self.has_multiple_yields {
            return source_order::TraversalSignal::Skip;
        }
        match node {
            AnyNodeRef::StmtTry(_) | AnyNodeRef::StmtIf(_) | AnyNodeRef::StmtMatch(_) => {
                self.branch_stack.push(vec![]);
            }
            AnyNodeRef::ElifElseClause(_)
            | AnyNodeRef::MatchCase(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_) => {
                // Save the yield count of previous branch
                let current = self.path_yields.pop().unwrap();
                if let Some(branch) = self.branch_stack.last_mut() {
                    branch.push(current);
                }
                // Start fresh path count for this branch
                self.path_yields.push(0);
            }
            AnyNodeRef::StmtFor(_) | AnyNodeRef::StmtWhile(_) => {
                // Yields in loops are at high risk of being executed multiple times
                self.has_multiple_yields = true;
            }
            _ => {}
        }
        source_order::TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        match node {
            AnyNodeRef::StmtTry(_) | AnyNodeRef::StmtIf(_) => {
                let current = self.path_yields.pop().unwrap();
                let mut branch = self.branch_stack.pop().unwrap_or_default();
                branch.push(current);
                let max_yield = branch.iter().max().copied().unwrap_or(0);

                if max_yield > 1 {
                    self.has_multiple_yields = true;
                }

                self.path_yields.push(max_yield);
            }
            AnyNodeRef::ElifElseClause(_) | AnyNodeRef::MatchCase(_) => {
                // Handled on enter/leave of outer structure
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                if let Some(count) = self.path_yields.last_mut() {
                    *count += 1;
                    if *count > 1 {
                        self.has_multiple_yields = true;
                    }
                }
            }
            _ => source_order::walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::FunctionDef(nested) => {
                function_def_visit_preorder_except_body(nested, self);
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }
}
