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
            if scope.yields_excessively() {
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

    fn propagate_yields(&mut self, yields: &[&'a Expr]) {
        self.report_excess(yields);
        let scope = self
            .scopes
            .last_mut()
            .expect("Scope stack should never be empty during AST traversal");
        for &yield_expr in yields {
            scope.add_yield(yield_expr);
        }
        let yields_excessive = scope.yields_excessively();
        let yield_exprs_clone = scope.yield_expressions.clone();
        if yields_excessive {
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

    fn max_yields(&self, branches: &[Vec<&'a Expr>]) -> Vec<&'a Expr> {
        branches
            .iter()
            .max_by_key(|branch| branch.len())
            .cloned()
            .unwrap_or_default()
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

    fn handle_exclusive_branches(&mut self, branch_count: usize) {
        let mut returning_branches = Vec::new();
        let mut continuing_branches = Vec::new();

        for _ in 0..branch_count {
            let (branch_yields, branch_returns) = self.pop_scope();
            self.report_excess(&branch_yields);

            if branch_returns {
                returning_branches.push(branch_yields);
            } else {
                continuing_branches.push(branch_yields);
            }
        }

        let max_returning = self.max_yields(&returning_branches);
        let max_continuing = self.max_yields(&continuing_branches);

        self.report_excess(&max_returning);
        self.propagate_yields(&max_continuing);
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

    fn yields_excessively(&self) -> bool {
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

                let mut returning_except_branches = Vec::new();
                let mut continuing_except_branches = Vec::new();

                for _ in 0..try_stmt.handlers.len() {
                    let (except_yields, except_returns) = self.pop_scope();
                    self.report_excess(&except_yields);

                    if except_returns {
                        returning_except_branches.push(except_yields);
                    } else {
                        continuing_except_branches.push(except_yields);
                    }
                }

                let (try_yields, try_returns) = self.pop_scope();

                self.report_excess(&try_yields);
                self.report_excess(&else_yields);
                self.report_excess(&finally_yields);

                if finally_returns {
                    let max_returning_except = self.max_yields(&returning_except_branches);
                    let max_continuing_except = self.max_yields(&continuing_except_branches);
                    let max_except_yields =
                        if max_returning_except.len() > max_continuing_except.len() {
                            max_returning_except
                        } else {
                            max_continuing_except
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
                    self.report_excess(&max_path);
                    // Clear current scope because finally returns
                    self.clear_yields_in_current_scope();
                } else {
                    // Finally doesn't return: we need to handle the different control flow paths and
                    // propagate yield count to outer scope.
                    // Since the code preceding yields is most likely to fail, we assume either
                    // valid try-else-finally or erroneous except-finally execution.

                    let max_returning_except = self.max_yields(&returning_except_branches);
                    let max_continuing_except = self.max_yields(&continuing_except_branches);

                    // Check except branches that return and don't propagate yields
                    let mut exception_return = max_returning_except;
                    exception_return.extend(finally_yields.clone());
                    self.report_excess(&exception_return);

                    let mut exception_no_return = max_continuing_except;
                    exception_no_return.extend(finally_yields.clone());

                    let mut normal_path = try_yields.clone();
                    normal_path.extend(else_yields);
                    normal_path.extend(finally_yields.clone());

                    // If try returns, we consider try-finally
                    // If try doesn't return, we consider try-(max of else OR non-return except)-finally
                    // Propagate yields from non-returning path
                    // Returning except branches are handled above
                    if try_returns {
                        let mut try_path = try_yields.clone();
                        // Finally is executed even if else returns
                        try_path.extend(finally_yields.clone());
                        self.report_excess(&try_path);

                        // Propagate the non-returning exception
                        self.propagate_yields(&exception_no_return);
                    } else {
                        if else_returns {
                            // Finally is executed even if else returns
                            // Check returning path and propagate the non-returning exception
                            self.report_excess(&normal_path);
                            self.propagate_yields(&exception_no_return);
                        } else {
                            // No returns, we propagate yields along the path with maximum yields
                            let max_yield_path = if normal_path.len() > exception_no_return.len() {
                                normal_path
                            } else {
                                exception_no_return
                            };
                            self.propagate_yields(&max_yield_path);
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
                self.propagate_yields(&else_yields);
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
