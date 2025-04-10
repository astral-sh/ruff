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
    yield_counts: Vec<usize>,
}

impl Default for YieldPathTracker {
    fn default() -> Self {
        Self {
            has_multiple_yields: false,
            yield_counts: vec![0],
        }
    }
}

impl<'a> source_order::SourceOrderVisitor<'a> for YieldPathTracker {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> source_order::TraversalSignal {
        if self.has_multiple_yields {
            return source_order::TraversalSignal::Skip;
        }
        match node {
            AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::ExceptHandlerExceptHandler(_)
            | AnyNodeRef::ElifElseClause(_)
            | AnyNodeRef::MatchCase(_) => {
                self.yield_counts.push(0);
            }
            AnyNodeRef::StmtFor(_) | AnyNodeRef::StmtWhile(_) => {
                // Yields in loops are at high risk of being executed multiple times
                self.has_multiple_yields = true;
                return source_order::TraversalSignal::Skip;
            }
            _ => {}
        }
        source_order::TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        match node {
            AnyNodeRef::StmtTry(try_stmt) => {
                let finally_yields = if try_stmt.finalbody.is_empty() {
                    0
                } else {
                    self.yield_counts.pop().unwrap_or(0)
                };

                let else_yields = if try_stmt.orelse.is_empty() {
                    0
                } else {
                    self.yield_counts.pop().unwrap_or(0)
                };

                let mut max_except_yields = 0;
                for _ in 0..try_stmt.handlers.len() {
                    let except_yields = self.yield_counts.pop().unwrap_or(0);
                    max_except_yields = max_except_yields.max(except_yields);
                }

                let try_yields = self.yield_counts.pop().unwrap_or(0);

                let try_except_finally = try_yields + max_except_yields + finally_yields;
                let try_else_finally = try_yields + else_yields + finally_yields;

                let max_path_yields = try_except_finally.max(try_else_finally);

                if let Some(root) = self.yield_counts.pop() {
                    self.yield_counts.push(root + max_path_yields);
                }
            }
            AnyNodeRef::StmtIf(if_stmt) => {
                let branch_counts = 1 + if_stmt.elif_else_clauses.len();

                let mut max_branch_yields = 0;
                for _ in 0..branch_counts {
                    let branch_yields = self.yield_counts.pop().unwrap_or(0);
                    max_branch_yields = max_branch_yields.max(branch_yields);
                }

                if let Some(root) = self.yield_counts.pop() {
                    self.yield_counts.push(root + max_branch_yields);
                }
            }
            AnyNodeRef::StmtMatch(match_stmt) => {
                let branch_counts = match_stmt.cases.len();
                let mut max_branch_yields = 0;

                for _ in 0..branch_counts {
                    let branch_yields = self.yield_counts.pop().unwrap_or(0);
                    max_branch_yields = max_branch_yields.max(branch_yields);
                }

                if let Some(root) = self.yield_counts.pop() {
                    self.yield_counts.push(root + max_branch_yields);
                }
            }
            AnyNodeRef::ElifElseClause(_) | AnyNodeRef::MatchCase(_) => {
                // Handled on enter/leave of outer structure
            }
            _ => {}
        }
        if let Some(count) = self.yield_counts.last() {
            if *count > 1 {
                self.has_multiple_yields = true;
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                if let Some(count) = self.yield_counts.last_mut() {
                    *count += 1;
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
            ast::Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                source_order::walk_body(self, body);
                for handler in handlers {
                    self.yield_counts.push(0);
                    source_order::walk_except_handler(self, handler);
                }
                self.yield_counts.push(0);
                source_order::walk_body(self, orelse);
                self.yield_counts.push(0);
                source_order::walk_body(self, finalbody);
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }
}
