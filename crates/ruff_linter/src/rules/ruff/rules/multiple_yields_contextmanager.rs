use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, visitor::source_order};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::function_def_visit_sourceorder_except_body;
use crate::{FixAvailability, Violation};

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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Function decorated with `contextlib.contextmanager` may yield more than once".to_string()
    }
}

/// RUF062
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
        checker.report_diagnostic(MultipleYieldsInContextManager, function_def.identifier());
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
    yield_stack: Vec<usize>,
    return_stack: Vec<bool>,
}

impl YieldPathTracker {
    fn would_increment_scope_yields(&mut self, by: usize) {
        match self.yield_stack.pop() {
            Some(scope_yields) => {
                if scope_yields + by > 1 {
                    self.has_multiple_yields = true;
                }
                self.yield_stack.push(scope_yields);
            }
            None => {
                debug_assert!(false, "Invalid yield stack size when traversing AST");
                self.yield_stack.push(0);
            }
        }
    }
    fn apply_increment_scope_yields(&mut self, by: usize) {
        match self.yield_stack.pop() {
            Some(scope_yields) => {
                let updated_scope_yields = scope_yields + by;
                if updated_scope_yields > 1 {
                    self.has_multiple_yields = true;
                }
                self.yield_stack.push(updated_scope_yields);
            }
            None => {
                self.yield_stack.push(by);
                debug_assert!(false, "Invalid yield stack size when traversing AST");
            }
        }
    }

    fn reset_scope_yields(&mut self) {
        match self.yield_stack.pop() {
            Some(root) => {
                if root > 1 {
                    self.has_multiple_yields = true;
                }
                self.yield_stack.push(0);
            }
            None => {
                self.yield_stack.push(0);
                debug_assert!(false, "Invalid yield stack size when traversing AST");
            }
        }
    }

    fn pop_yields(&mut self) -> usize {
        self.yield_stack.pop().unwrap_or_else(|| {
            debug_assert!(false, "Invalid yield stack size when traversing AST");
            0
        })
    }

    fn pop_returns(&mut self) -> bool {
        self.return_stack.pop().unwrap_or_else(|| {
            debug_assert!(false, "Invalid return stack size when traversing AST");
            false
        })
    }

    fn handle_exclusive_branches(&mut self, branch_count: usize) {
        let mut max_yields_return_branches = 0;
        let mut max_yields_no_return_branches = 0;
        for _ in 0..branch_count {
            let branch_yields = self.pop_yields();
            let branch_returns = self.pop_returns();
            if branch_returns {
                max_yields_return_branches = max_yields_return_branches.max(branch_yields);
            } else {
                max_yields_no_return_branches = max_yields_no_return_branches.max(branch_yields);
            }
        }
        self.would_increment_scope_yields(max_yields_return_branches);
        self.apply_increment_scope_yields(max_yields_no_return_branches);
    }

    fn push_new_scope(&mut self) {
        self.yield_stack.push(0);
        self.return_stack.push(false);
    }
}

impl Default for YieldPathTracker {
    fn default() -> Self {
        Self {
            has_multiple_yields: false,
            yield_stack: vec![0],
            return_stack: vec![false],
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
                // Track for primary control flow structures
                // Optional branches like else/finally clauses are handled in leave_node
                // Except is handled in leave node to maintain logical locality
                self.push_new_scope();
            }
            _ => {}
        }
        source_order::TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'a>) {
        match node {
            AnyNodeRef::StmtTry(try_stmt) => {
                // Finally is always executed, even if prior branches return
                // Other branches are skipped
                let finally_yields = self.pop_yields();
                let finally_returns = self.pop_returns();

                let else_yields = self.pop_yields();
                let else_returns = self.pop_returns();

                // We need to distinguish whether an except branch returns
                let mut max_except_yields_with_return = 0;
                let mut max_except_yields_no_return = 0;
                for _ in 0..try_stmt.handlers.len() {
                    let except_yields = self.pop_yields();
                    let except_returns = self.pop_returns();
                    if except_returns {
                        max_except_yields_with_return =
                            max_except_yields_with_return.max(except_yields);
                    } else {
                        max_except_yields_no_return =
                            max_except_yields_no_return.max(except_yields);
                    }
                }
                let max_except_yields =
                    max_except_yields_no_return.max(max_except_yields_with_return);

                let try_yields = self.pop_yields();
                let try_returns = self.pop_returns();

                if finally_returns {
                    // Finally always executes; can ignore earlier returns
                    let max_path_yields =
                        try_yields + max_except_yields.max(else_yields) + finally_yields;
                    self.would_increment_scope_yields(max_path_yields);
                    self.reset_scope_yields();
                } else {
                    // Since the code preceding yields is most likely to fail, we assume either
                    // valid try-else-finally or erroneous except-finally execution.
                    // Distinguish returning and non-returning except for propagation.

                    let exception_return = max_except_yields_with_return + finally_yields;
                    let exception_no_return = max_except_yields_no_return + finally_yields;
                    let valid_try_else_finally = try_yields + else_yields + finally_yields;

                    // Probe exceptions with returns for all possibilities
                    self.would_increment_scope_yields(exception_return);

                    if try_returns {
                        let valid_try_return = try_yields + finally_yields;
                        self.would_increment_scope_yields(valid_try_return);
                        // Propagate the non-returning exception
                        self.apply_increment_scope_yields(exception_no_return);
                    } else {
                        // Finally is executed even if else returns
                        self.would_increment_scope_yields(valid_try_else_finally);
                        if else_returns {
                            // Propagate the non-returning exception
                            self.apply_increment_scope_yields(exception_no_return);
                        } else {
                            self.apply_increment_scope_yields(
                                valid_try_else_finally.max(exception_no_return),
                            );
                        }
                    }
                }
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
                let else_returns = self.pop_returns();
                let body_yields = self.pop_yields();
                let _body_returns = self.pop_returns();

                // Yield in loop is likely to yield multiple times
                self.has_multiple_yields |= body_yields > 0;
                self.apply_increment_scope_yields(else_yields);
                if else_returns {
                    // If else returns, don't propagate yield count
                    self.reset_scope_yields();
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        match expr {
            ast::Expr::Yield(_) | ast::Expr::YieldFrom(_) => {
                if let Some(count) = self.yield_stack.last_mut() {
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
            ast::Stmt::Return(_) => {
                if let Some(returns) = self.return_stack.last_mut() {
                    *returns = true;
                }
            }
            ast::Stmt::FunctionDef(nested) => {
                function_def_visit_sourceorder_except_body(nested, self);
            }
            ast::Stmt::While(loop_stmt @ ast::StmtWhile { body, orelse, .. }) => {
                let node = ruff_python_ast::AnyNodeRef::StmtWhile(loop_stmt);

                if self.enter_node(node).is_traverse() {
                    self.visit_body(body);
                    self.push_new_scope();
                    self.visit_body(orelse);
                    self.leave_node(node);
                }
            }
            ast::Stmt::For(loop_stmt @ ast::StmtFor { body, orelse, .. }) => {
                let node = ruff_python_ast::AnyNodeRef::StmtFor(loop_stmt);
                if self.enter_node(node).is_traverse() {
                    self.visit_body(body);
                    self.push_new_scope();
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
                        self.push_new_scope();
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
                        self.push_new_scope();
                        self.visit_except_handler(handler);
                    }

                    self.push_new_scope();
                    self.visit_body(orelse);
                    self.push_new_scope();
                    self.visit_body(finalbody);
                    self.leave_node(node);
                }
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }
}
