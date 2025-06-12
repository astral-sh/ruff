use ast::{Expr, StmtFunctionDef};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{self as ast, visitor::source_order};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;

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
pub(crate) struct MultipleYieldsInContextManager {
    decorator_name: String,
}

impl MultipleYieldsInContextManager {
    fn new(decorator_name: String) -> Self {
        Self { decorator_name }
    }
}

impl Violation for MultipleYieldsInContextManager {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Function decorated with `{}` may yield more than once",
            self.decorator_name
        )
    }
}

/// RUF062
pub(crate) fn multiple_yields_in_contextmanager(checker: &Checker, function_def: &StmtFunctionDef) {
    if let Some(context_manager_name) = get_contextmanager_decorator(function_def, checker) {
        let mut path_tracker = YieldPathTracker::new();
        source_order::walk_body(&mut path_tracker, &function_def.body);
        for expr in path_tracker.into_violations() {
            checker.report_diagnostic(
                MultipleYieldsInContextManager::new(context_manager_name.clone()),
                expr.range(),
            );
        }
    }
}

fn get_contextmanager_decorator(
    function_def: &StmtFunctionDef,
    checker: &Checker,
) -> Option<String> {
    function_def.decorator_list.iter().find_map(|decorator| {
        let callable = map_callable(&decorator.expression);
        checker
            .semantic()
            .resolve_qualified_name(callable)
            .and_then(|qualified| match qualified.segments() {
                ["contextlib", "contextmanager"] => Some("contextlib.contextmanager".to_string()),
                ["contextlib", "asynccontextmanager"] => {
                    Some("contextlib.asynccontextmanager".to_string())
                }
                _ => None,
            })
    })
}

struct YieldPathTracker<'a> {
    yield_stack: Vec<Vec<&'a Expr>>,
    return_stack: Vec<bool>,
    violations: FxHashMap<TextRange, &'a Expr>,
}

impl<'a> YieldPathTracker<'a> {
    fn new() -> Self {
        Self {
            yield_stack: vec![Vec::new()],
            return_stack: vec![false],
            violations: FxHashMap::default(),
        }
    }

    fn check_terminating_branch(&mut self, yields: Vec<&'a Expr>) {
        if yields.len() > 1 {
            self.report_multiple_yield_violations(&yields);
        }
    }

    fn merge_continuing_branch(&mut self, yields: Vec<&'a Expr>) {
        if yields.len() > 1 {
            self.report_multiple_yield_violations(&yields);
        }
        let current_scope_len = if let Some(current_scope) = self.yield_stack.last_mut() {
            current_scope.extend(yields);
            current_scope.len()
        } else {
            0
        };
        if current_scope_len > 1 {
            self.report_multiple_yield_violations(&self.yield_stack.last().unwrap().clone());
        }
    }

    fn clear_current_yield_scope(&mut self) {
        if let Some(current_scope) = self.yield_stack.last_mut() {
            current_scope.clear();
        } else {
            debug_assert!(false, "Invalid yield stack size when traversing AST");
            self.yield_stack.push(Vec::new());
        }
    }

    fn get_current_scope_yields(&mut self) -> Vec<&'a Expr> {
        self.yield_stack.pop().unwrap_or_else(|| {
            debug_assert!(false, "Invalid yield stack size when traversing AST");
            Vec::new()
        })
    }

    fn current_scope_returns(&mut self) -> bool {
        self.return_stack.pop().unwrap_or_else(|| {
            debug_assert!(false, "Invalid return stack size when traversing AST");
            false
        })
    }

    fn report_multiple_yield_violations(&mut self, yields: &[&'a Expr]) {
        // Only report the second to last violation
        for &yield_expr in yields.iter().skip(1) {
            self.violations.insert(yield_expr.range(), yield_expr);
        }
    }

    fn report_single_yield_violation(&mut self, yield_expr: &'a Expr) {
        self.violations.insert(yield_expr.range(), yield_expr);
    }

    fn into_violations(self) -> impl Iterator<Item = &'a Expr> {
        self.violations.into_values()
    }

    // For exclusive branches (if/elif/else, match cases, ...) - propagate the maximum
    fn handle_exclusive_branches(&mut self, branch_count: usize) {
        let mut max_yields_in_returning_branches = Vec::new();
        let mut max_yields_in_nonreturning_branches = Vec::new();
        for _ in 0..branch_count {
            let branch_yields = self.get_current_scope_yields();
            let branch_returns = self.current_scope_returns();

            if branch_yields.len() > 1 {
                self.report_multiple_yield_violations(&branch_yields);
            }

            if branch_returns {
                if branch_yields.len() > max_yields_in_returning_branches.len() {
                    max_yields_in_returning_branches = branch_yields;
                }
            } else {
                if branch_yields.len() > max_yields_in_nonreturning_branches.len() {
                    max_yields_in_nonreturning_branches = branch_yields;
                }
            }
        }
        self.check_terminating_branch(max_yields_in_returning_branches);
        self.merge_continuing_branch(max_yields_in_nonreturning_branches);
    }

    fn push_new_scope(&mut self) {
        self.yield_stack.push(Vec::<&'a Expr>::new());
        self.return_stack.push(false);
    }
}

impl<'a> source_order::SourceOrderVisitor<'a> for YieldPathTracker<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> source_order::TraversalSignal {
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
                let finally_yields = self.get_current_scope_yields();
                let finally_returns = self.current_scope_returns();

                let else_yields = self.get_current_scope_yields();
                let else_returns = self.current_scope_returns();

                // We need to distinguish whether an except branch returns
                let mut max_yields_in_returning_except_branch = Vec::new();
                let mut max_yields_in_nonreturning_except_branch = Vec::new();

                for _ in 0..try_stmt.handlers.len() {
                    let except_yields = self.get_current_scope_yields();
                    let except_returns = self.current_scope_returns();

                    if except_yields.len() > 1 {
                        self.report_multiple_yield_violations(&except_yields);
                    }

                    if except_returns {
                        if except_yields.len() > max_yields_in_returning_except_branch.len() {
                            max_yields_in_returning_except_branch = except_yields;
                        }
                    } else {
                        if except_yields.len() > max_yields_in_nonreturning_except_branch.len() {
                            max_yields_in_nonreturning_except_branch = except_yields;
                        }
                    }
                }

                let try_yields = self.get_current_scope_yields();
                let try_returns = self.current_scope_returns();

                if try_yields.len() > 1 {
                    self.report_multiple_yield_violations(&try_yields);
                }
                if else_yields.len() > 1 {
                    self.report_multiple_yield_violations(&else_yields);
                }
                if finally_yields.len() > 1 {
                    self.report_multiple_yield_violations(&finally_yields);
                }

                if finally_returns {
                    // Get maximum accumulated yields in all except branches
                    let max_except_yields = if max_yields_in_returning_except_branch.len()
                        > max_yields_in_returning_except_branch.len()
                    {
                        max_yields_in_returning_except_branch
                    } else {
                        max_yields_in_nonreturning_except_branch
                    };

                    // We need to consider all possible paths through try/except/else/finally
                    let mut common_path = try_yields.clone();
                    let mut max_path = if !try_returns {
                        // try + (else OR except) + finally
                        // Else is only executed if no exception
                        common_path.extend(if else_yields.len() > max_except_yields.len() {
                            else_yields
                        } else {
                            max_except_yields
                        });
                        common_path
                    } else {
                        common_path
                    };
                    // Finally always executes, even when previous branches return
                    max_path.extend(finally_yields.clone());

                    // This branch terminates because finally returns
                    self.check_terminating_branch(max_path);
                    // Clear current scope because finally returns
                    self.clear_current_yield_scope();
                } else {
                    // Finally doesn't return: we need to handle the different control flow paths and
                    // propagate yield count to outer scope.
                    // Since the code preceding yields is most likely to fail, we assume either
                    // valid try-else-finally or erroneous except-finally execution.

                    // Check except branches that return and don't propagate yields
                    let mut exception_return = max_yields_in_returning_except_branch;
                    exception_return.extend(finally_yields.clone());
                    self.check_terminating_branch(exception_return);

                    let mut exception_no_return = max_yields_in_nonreturning_except_branch;
                    exception_no_return.extend(finally_yields.clone());

                    let mut valid_try_else_finally = try_yields.clone();
                    valid_try_else_finally.extend(else_yields);
                    valid_try_else_finally.extend(finally_yields.clone());

                    // If try returns, we consider try-finally
                    // If try doesn't return, we consider try-(max of else OR non-return except)-finally
                    // Propagate yields from non-returning path
                    // Returning except branches are handled above
                    if try_returns {
                        let mut valid_try_return = try_yields.clone();
                        // Finally is executed even if else returns
                        valid_try_return.extend(finally_yields.clone());
                        self.check_terminating_branch(valid_try_return);

                        // Propagate the non-returning exception
                        self.merge_continuing_branch(exception_no_return);
                    } else {
                        if else_returns {
                            // Finally is executed even if else returns
                            // Check returning path and propagate the non-returning exception
                            self.check_terminating_branch(valid_try_else_finally);
                            self.merge_continuing_branch(exception_no_return);
                        } else {
                            // No returns, we propagate yields along the path with maximum yields
                            let max_yield_path =
                                if valid_try_else_finally.len() > exception_no_return.len() {
                                    valid_try_else_finally
                                } else {
                                    exception_no_return
                                };
                            self.merge_continuing_branch(max_yield_path);
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
                let else_yields = self.get_current_scope_yields();
                let else_returns = self.current_scope_returns();
                let body_yields = self.get_current_scope_yields();
                let _body_returns = self.current_scope_returns();

                // Without an unconditional break yield in loop is likely to yield multiple times
                if body_yields.len() > 0 {
                    // TODO(maxmynter): Only report when no unconditional `break` in loop
                    if body_yields.len() == 1 {
                        self.report_single_yield_violation(body_yields.first().unwrap());
                    } else {
                        self.report_multiple_yield_violations(&body_yields);
                    }
                }
                self.merge_continuing_branch(else_yields);
                if else_returns {
                    // If else returns, don't propagate yield count
                    self.clear_current_yield_scope();
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                if let Some(current_scope_yields) = self.yield_stack.last_mut() {
                    current_scope_yields.push(expr);
                    if current_scope_yields.len() > 1 {
                        self.violations.insert(expr.range(), expr);
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
