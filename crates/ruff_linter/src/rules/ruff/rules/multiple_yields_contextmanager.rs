use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{self as ast, visitor::source_order};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::function_def_visit_preorder_except_body;

/// ## What it does
/// Checks that a function decorated with `contextlib.contextmanager` yields at most once.
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
    function_def.decorator_list.iter().any(|decorator| {
        let callable = map_callable(&decorator.expression);
        checker
            .semantic()
            .resolve_qualified_name(callable)
            .is_some_and(|qualified| {
                matches!(
                    qualified.segments(),
                    ["contextlib", "contextmanager" | "asynccontextmanager"]
                )
            })
    })
}

struct YieldPathTracker {
    has_multiple_yields: bool,
    yield_counts: Vec<usize>,
}

impl YieldPathTracker {
    fn increment_yield_counts_top_by(&mut self, by: usize) {
        match self.yield_counts.pop() {
            Some(root) => {
                let new_root = root + by;
                if new_root > 1 {
                    self.has_multiple_yields = true;
                }
                self.yield_counts.push(new_root);
            }
            None => {
                self.yield_counts.push(by);
                debug_assert!(false, "Invalid yield stack size when traversing AST");
            }
        }
    }

    fn pop_yields(&mut self) -> usize {
        match self.yield_counts.pop() {
            Some(counts) => counts,
            None => {
                debug_assert!(false, "Invalid yield stack size when traversing AST");
                0
            }
        }
    }

    fn handle_exclusive_branches(&mut self, branch_count: usize) {
        let mut max_yields_branches = 0;
        for _ in 0..branch_count {
            let branch_yields = self.pop_yields();
            max_yields_branches = max_yields_branches.max(branch_yields);
        }
        self.increment_yield_counts_top_by(max_yields_branches);
    }
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
            AnyNodeRef::StmtFor(_)
            | AnyNodeRef::StmtWhile(_)
            | AnyNodeRef::StmtTry(_)
            | AnyNodeRef::StmtIf(_)
            | AnyNodeRef::StmtMatch(_)
            | AnyNodeRef::MatchCase(_) => {
                // Track yields for primary control flow structures
                // Optional branches like else/finally clauses are handled in leave_node
                // Except is handled in leave node to maintain logical locality
                self.yield_counts.push(0);
            }
            _ => {}
        }
        source_order::TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        match node {
            AnyNodeRef::StmtTry(try_stmt) => {
                let finally_yields = self.pop_yields();
                let else_yields = self.pop_yields();

                let mut max_except_yields = 0;
                for _ in 0..try_stmt.handlers.len() {
                    let except_yields = self.pop_yields();
                    max_except_yields = max_except_yields.max(except_yields);
                }

                let try_yields = self.pop_yields();

                let max_path_yields =
                    try_yields + max_except_yields.max(else_yields) + finally_yields;

                self.increment_yield_counts_top_by(max_path_yields);
            }
            AnyNodeRef::StmtIf(if_stmt) => {
                let branch_count = 1 + if_stmt.elif_else_clauses.len();
                self.handle_exclusive_branches(branch_count);
            }
            AnyNodeRef::StmtMatch(match_stmt) => {
                let branch_count = match_stmt.cases.len();
                self.handle_exclusive_branches(branch_count);
            }
            AnyNodeRef::StmtFor(_) | AnyNodeRef::StmtWhile(_) => {
                let else_yields = self.pop_yields();
                let body_yields = self.pop_yields();

                // Yield in loop is likely to yield multiple times
                self.has_multiple_yields |= body_yields > 0;
                self.increment_yield_counts_top_by(else_yields);
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                if let Some(count) = self.yield_counts.last_mut() {
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
            ast::Stmt::While(loop_stmt @ ast::StmtWhile { body, orelse, .. }) => {
                let node = ruff_python_ast::AnyNodeRef::StmtWhile(loop_stmt);

                if self.enter_node(node).is_traverse() {
                    self.visit_body(body);
                    self.yield_counts.push(0);
                    self.visit_body(orelse);
                    self.leave_node(node);
                }
            }
            ast::Stmt::For(loop_stmt @ ast::StmtFor { body, orelse, .. }) => {
                let node = ruff_python_ast::AnyNodeRef::StmtFor(loop_stmt);
                if self.enter_node(node).is_traverse() {
                    self.visit_body(body);
                    self.yield_counts.push(0);
                    self.visit_body(orelse);
                    self.leave_node(node);
                }
            }
            ast::Stmt::If(
                if_stmt @ ast::StmtIf {
                    body,
                    elif_else_clauses,
                    ..
                },
            ) => {
                let node = ruff_python_ast::AnyNodeRef::StmtIf(if_stmt);
                if self.enter_node(node).is_traverse() {
                    self.visit_body(body);
                    for clause in elif_else_clauses {
                        self.yield_counts.push(0);
                        self.visit_elif_else_clause(clause);
                    }
                    self.leave_node(node);
                }
            }
            ast::Stmt::Try(
                try_stmt @ ast::StmtTry {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                    ..
                },
            ) => {
                let node = ruff_python_ast::AnyNodeRef::StmtTry(try_stmt);
                if self.enter_node(node).is_traverse() {
                    self.visit_body(body);
                    for handler in handlers {
                        self.yield_counts.push(0);
                        self.visit_except_handler(handler);
                    }
                    self.yield_counts.push(0);
                    self.visit_body(orelse);
                    self.yield_counts.push(0);
                    self.visit_body(finalbody);
                    self.leave_node(node);
                }
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }
}
