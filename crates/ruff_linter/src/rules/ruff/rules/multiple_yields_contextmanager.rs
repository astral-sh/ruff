use ast::{Expr, StmtFunctionDef};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{
    self as ast,
    visitor::source_order::{self, SourceOrderVisitor},
};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

use crate::checkers::ast::Checker;
use crate::rules::ruff::helpers::function_def_visit_sourceorder_except_body;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks that a function decorated with `contextlib.contextmanager` or `contextlib.asynccontextmanager` yields at most once.
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
///
/// Use instead:
/// ```python
/// @contextlib.contextmanager
/// def good_context_manager():
///     print("Setting up")
///     yield "only value"  # Single yield is correct
///     print("Cleanup")
/// ```
/// ## References
/// - [Python documentation: contextlib.contextmanager](https://docs.python.org/3/library/contextlib.html#contextlib.contextmanager)
/// - [Python documentation: contextlib.asynccontextmanager](https://docs.python.org/3/library/contextlib.html#contextlib.asynccontextmanager)
#[derive(ViolationMetadata)]
pub(crate) struct MultipleYieldsInContextManager {
    decorator_name: &'static str,
}

impl MultipleYieldsInContextManager {
    fn new(decorator_name: &'static str) -> Self {
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
        let mut violations = Vec::new();
        {
            let mut yield_tracker = YieldTracker::new(&mut violations);
            source_order::walk_body(&mut yield_tracker, &function_def.body);
        }

        for range in violations {
            checker.report_diagnostic(
                MultipleYieldsInContextManager::new(context_manager_name),
                range,
            );
        }
    }
}

fn get_contextmanager_decorator(
    function_def: &StmtFunctionDef,
    checker: &Checker,
) -> Option<&'static str> {
    function_def.decorator_list.iter().find_map(|decorator| {
        let callable = map_callable(&decorator.expression);
        checker
            .semantic()
            .resolve_qualified_name(callable)
            .and_then(|qualified| match qualified.segments() {
                ["contextlib", "contextmanager"] => Some("contextlib.contextmanager"),
                ["contextlib", "asynccontextmanager"] => Some("contextlib.asynccontextmanager"),
                _ => None,
            })
    })
}

// YieldTracker tracks yield expressions along the control flow path.
// If we encounter multiple yields in a single path, the contextmanager protocol is broken
// and we collect violations to be emitted later.
//
// The tracker maintains a stack of scopes that contain the scope yield expressions
// and whether the scope returns (to determine if we need to continue traversing the path).
// Within a scope we evaluate all control flow paths and propagate the yields along the
// maximum path to the outer scope.
// Return exits the contextmanager decorated function and we stop accumulating yields along that path.
struct YieldTracker<'a> {
    violations: &'a mut Vec<TextRange>,
    scopes: Vec<YieldScope<'a>>,
    reported_ranges: FxHashSet<TextRange>,
}

impl<'a> YieldTracker<'a> {
    fn new(violations: &'a mut Vec<TextRange>) -> Self {
        Self {
            violations,
            scopes: vec![YieldScope::new()],
            reported_ranges: FxHashSet::default(),
        }
    }

    fn add_yield(&mut self, expr: &'a Expr) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.add_yield(expr);
            if scope.does_yield_more_than_once() {
                self.emit_violation(expr);
            }
        }
    }

    fn emit_violation(&mut self, expr: &'a Expr) {
        let range = expr.range();
        if self.reported_ranges.insert(range) {
            self.violations.push(range);
        }
    }

    fn check_terminating_branch(&mut self, yields: &[&'a Expr]) {
        self.report_excess(yields);
    }

    fn merge_continuing_branch(&mut self, yields: &[&'a Expr]) {
        self.report_excess(yields);
        let scope = self
            .scopes
            .last_mut()
            .expect("Scope stack should never be empty during AST traversal");
        for &yield_expr in yields {
            scope.add_yield(yield_expr);
        }
        let does_yield_more_than_once = scope.does_yield_more_than_once();
        let yield_exprs_clone = scope.yield_expressions.clone();
        if does_yield_more_than_once {
            self.emit_multiple_violations(&yield_exprs_clone);
        }
    }

    fn emit_multiple_violations(&mut self, yields: &[&'a Expr]) {
        // Only report the second to last violations
        for &yield_expr in yields.iter().skip(1) {
            self.emit_violation(yield_expr);
        }
    }

    fn report_excess(&mut self, yields: &[&'a Expr]) {
        if yields.len() > 1 {
            self.emit_multiple_violations(yields);
        }
    }

    fn clear_yields_in_current_scope(&mut self) {
        self.scopes
            .last_mut()
            .expect("Scope stack should never be empty during AST traversal")
            .clear();
    }

    fn pop_scope(&mut self) -> (Vec<&'a Expr>, bool) {
        let scope = self
            .scopes
            .pop()
            .expect("Scope stack should never be empty during AST traversal");
        (scope.yield_expressions, scope.does_return)
    }

    fn push_scope(&mut self, scope: YieldScope<'a>) {
        self.scopes.push(scope);
    }

    fn handle_loop(&mut self, body: &'a [ast::Stmt], orelse: &'a [ast::Stmt]) {
        self.visit_body(body);
        self.push_scope(YieldScope::new());
        self.visit_body(orelse);
    }

    // For exclusive branches propagate maximum yield count
    fn handle_exclusive_branches(&mut self, branch_count: usize) {
        let mut max_yields_in_returning_branches = Vec::new();
        let mut max_yields_in_nonreturning_branches = Vec::new();
        for _ in 0..branch_count {
            let (branch_yields, branch_returns) = self.pop_scope();

            self.report_excess(&branch_yields);

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
        self.check_terminating_branch(&max_yields_in_returning_branches);
        self.merge_continuing_branch(&max_yields_in_nonreturning_branches);
    }
}

struct YieldScope<'a> {
    yield_expressions: Vec<&'a Expr>,
    does_return: bool,
}

impl<'a> YieldScope<'a> {
    fn new() -> Self {
        Self {
            yield_expressions: Vec::new(),
            does_return: false,
        }
    }

    fn clear(&mut self) {
        self.yield_expressions.clear();
    }

    fn does_yield_more_than_once(&self) -> bool {
        self.yield_expressions.len() > 1
    }

    fn add_yield(&mut self, expr: &'a Expr) {
        self.yield_expressions.push(expr);
    }

    fn set_does_return(&mut self, value: bool) {
        self.does_return = value;
    }
}

impl<'a> source_order::SourceOrderVisitor<'a> for YieldTracker<'a> {
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
                self.push_scope(YieldScope::new());
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
                let (finally_yields, finally_returns) = self.pop_scope();

                let (else_yields, else_returns) = self.pop_scope();

                // We need to distinguish whether an except branch returns
                let mut max_yields_in_returning_except_branch = Vec::new();
                let mut max_yields_in_nonreturning_except_branch = Vec::new();

                for _ in 0..try_stmt.handlers.len() {
                    let (except_yields, except_returns) = self.pop_scope();

                    self.report_excess(&except_yields);

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

                let (try_yields, try_returns) = self.pop_scope();

                self.report_excess(&try_yields);
                self.report_excess(&else_yields);
                self.report_excess(&finally_yields);

                if finally_returns {
                    // Get maximum accumulated yields in all except branches
                    let max_except_yields = if max_yields_in_returning_except_branch.len()
                        > max_yields_in_nonreturning_except_branch.len()
                    {
                        max_yields_in_returning_except_branch
                    } else {
                        max_yields_in_nonreturning_except_branch
                    };

                    // We need to consider all possible paths through try/except/else/finally
                    let mut common_path = try_yields.clone();
                    let mut max_path = if try_returns {
                        common_path
                    } else {
                        // try + (else OR except) + finally
                        // Else is only executed if no exception
                        common_path.extend(if else_yields.len() > max_except_yields.len() {
                            else_yields
                        } else {
                            max_except_yields
                        });
                        common_path
                    };
                    // Finally always executes, even when previous branches return
                    max_path.extend(finally_yields.clone());

                    // This branch terminates because finally returns
                    self.check_terminating_branch(&max_path);
                    // Clear current scope because finally returns
                    self.clear_yields_in_current_scope();
                } else {
                    // Finally doesn't return: we need to handle the different control flow paths and
                    // propagate yield count to outer scope.
                    // Since the code preceding yields is most likely to fail, we assume either
                    // valid try-else-finally or erroneous except-finally execution.

                    // Check except branches that return and don't propagate yields
                    let mut exception_return = max_yields_in_returning_except_branch;
                    exception_return.extend(finally_yields.clone());
                    self.check_terminating_branch(&exception_return);

                    let mut exception_no_return = max_yields_in_nonreturning_except_branch;
                    exception_no_return.extend(finally_yields.clone());

                    let mut try_else_finally = try_yields.clone();
                    try_else_finally.extend(else_yields);
                    try_else_finally.extend(finally_yields.clone());

                    // If try returns, we consider try-finally
                    // If try doesn't return, we consider try-(max of else OR non-return except)-finally
                    // Propagate yields from non-returning path
                    // Returning except branches are handled above
                    if try_returns {
                        let mut valid_try_return = try_yields.clone();
                        // Finally is executed even if else returns
                        valid_try_return.extend(finally_yields.clone());
                        self.check_terminating_branch(&valid_try_return);

                        // Propagate the non-returning exception
                        self.merge_continuing_branch(&exception_no_return);
                    } else {
                        if else_returns {
                            // Finally is executed even if else returns
                            // Check returning path and propagate the non-returning exception
                            self.check_terminating_branch(&try_else_finally);
                            self.merge_continuing_branch(&exception_no_return);
                        } else {
                            // No returns, we propagate yields along the path with maximum yields
                            let max_yield_path =
                                if try_else_finally.len() > exception_no_return.len() {
                                    try_else_finally
                                } else {
                                    exception_no_return
                                };
                            self.merge_continuing_branch(&max_yield_path);
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
                let (else_yields, else_returns) = self.pop_scope();
                let (body_yields, _body_returns) = self.pop_scope();

                if !body_yields.is_empty() {
                    if body_yields.len() == 1 {
                        self.emit_violation(body_yields.first().unwrap());
                    } else {
                        self.emit_multiple_violations(&body_yields);
                    }
                }
                self.merge_continuing_branch(&else_yields);
                if else_returns {
                    // If else returns, don't propagate yield count
                    self.clear_yields_in_current_scope();
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                self.add_yield(expr);
            }
            _ => source_order::walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::Return(_) => {
                if let Some(scope) = self.scopes.last_mut() {
                    scope.set_does_return(true);
                }
            }
            ast::Stmt::FunctionDef(nested) => {
                function_def_visit_sourceorder_except_body(nested, self);
            }
            ast::Stmt::While(loop_stmt @ ast::StmtWhile { body, orelse, .. }) => {
                let node = ruff_python_ast::AnyNodeRef::StmtWhile(loop_stmt);
                if self.enter_node(node).is_traverse() {
                    self.handle_loop(body, orelse);
                    self.leave_node(node);
                }
            }
            ast::Stmt::For(loop_stmt @ ast::StmtFor { body, orelse, .. }) => {
                let node = ruff_python_ast::AnyNodeRef::StmtFor(loop_stmt);
                if self.enter_node(node).is_traverse() {
                    self.handle_loop(body, orelse);
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
                        self.push_scope(YieldScope::new());
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
                        self.push_scope(YieldScope::new());
                        self.visit_except_handler(handler);
                    }

                    self.push_scope(YieldScope::new());
                    self.visit_body(orelse);
                    self.push_scope(YieldScope::new());
                    self.visit_body(finalbody);
                    self.leave_node(node);
                }
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }
}
