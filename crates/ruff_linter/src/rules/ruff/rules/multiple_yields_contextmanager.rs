use ast::{Expr, StmtFunctionDef};
use ruff_macros::{ViolationMetadata, derive_message_formats};
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
        let mut yield_tracker = YieldTracker::new(checker, context_manager_name);
        source_order::walk_body(&mut yield_tracker, &function_def.body);
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

// YieldTracker accumulates YieldScopes according to control flow
// If we encounter multiple yields in a single execution path, the contextmanager protocol is broken and we emit a diagnostic
//
// Within a scope we evaluate all control flow paths and propagate the yields along the
// maximum path to the outer scope.
// Return exits the contextmanager decorated function and we stop accumulating yields along that path.
#[derive(Clone, Copy)]
struct YieldScope {
    pub yield_count: usize,
    pub nested_in_loop: bool,
    pub returned: bool,
}

impl YieldScope {
    fn new(preceding_yields: usize, nested_in_loop: bool) -> Self {
        YieldScope {
            yield_count: preceding_yields,
            returned: false,
            nested_in_loop,
        }
    }

    fn from(preceding_yields: usize, returned: bool, nested_in_loop: bool) -> Self {
        YieldScope {
            yield_count: preceding_yields,
            nested_in_loop,
            returned,
        }
    }
}

struct YieldTracker<'a, 'b> {
    checker: &'a Checker<'b>,
    decorator_name: &'static str,
    scopes: Vec<YieldScope>,
    reported_ranges: FxHashSet<TextRange>,
}

impl<'a, 'b> YieldTracker<'a, 'b> {
    fn new(checker: &'a Checker<'b>, decorator_name: &'static str) -> Self {
        Self {
            checker,
            decorator_name,
            scopes: vec![YieldScope::new(0, false)],
            reported_ranges: FxHashSet::default(),
        }
    }

    fn enter_scope(&mut self, parent_scope: YieldScope) {
        self.scopes.push(YieldScope::new(
            parent_scope.yield_count,
            parent_scope.nested_in_loop,
        ));
    }

    fn exit_scope(&mut self) -> (usize, bool) {
        let scope = self.scopes.pop().unwrap();
        (scope.yield_count, scope.returned)
    }

    fn emit_violation_at_range(&mut self, range: TextRange) {
        if self.reported_ranges.insert(range) {
            self.checker.report_diagnostic(
                MultipleYieldsInContextManager::new(self.decorator_name),
                range,
            );
        }
    }

    fn handle_if_stmt(&mut self, if_stmt: &'a ast::StmtIf) {
        let parent_scope = *self.scopes.last().unwrap();
        let mut branch_counts = Vec::new();

        // Main if branch
        self.enter_scope(parent_scope);
        self.visit_body(&if_stmt.body);
        branch_counts.push(self.exit_scope());

        // elif/else branches
        for clause in &if_stmt.elif_else_clauses {
            self.enter_scope(parent_scope);
            self.visit_body(&clause.body);
            branch_counts.push(self.exit_scope());
        }

        let has_else = if_stmt
            .elif_else_clauses
            .last()
            .is_some_and(|clause| clause.test.is_none());

        let continuing: Vec<_> = branch_counts
            .iter()
            .filter_map(|(count, returns)| if *returns { None } else { Some(*count) })
            .collect();

        // Update parent scope with
        if let Some(scope) = self.scopes.last_mut() {
            if continuing.is_empty() {
                // If all branches return, check if else branch is among these.
                // Otherwise, returns are circumvented if all conditions are false
                scope.returned = has_else;
            } else {
                scope.yield_count = *continuing.iter().max().unwrap();
            }
        }
    }

    fn handle_loop(&mut self, body: &'a [ast::Stmt], orelse: &'a [ast::Stmt]) {
        let parent_scope = *self.scopes.last().unwrap();

        let mut loop_body_scope = parent_scope;
        loop_body_scope.nested_in_loop = true;

        self.enter_scope(loop_body_scope);
        self.visit_body(body);
        let (loop_count, loop_returns) = self.exit_scope();

        let (else_count, else_returns) = if loop_returns {
            (loop_count, false) // else doesn't add additional yields if loop returns
        } else {
            // Else clause only runs if loop doesn't return or break
            let else_parent = YieldScope::from(loop_count, false, parent_scope.nested_in_loop);
            self.enter_scope(else_parent);
            self.visit_body(orelse);
            self.exit_scope()
        };

        if let Some(scope) = self.scopes.last_mut() {
            scope.yield_count = else_count;
            if else_returns || loop_returns {
                scope.returned = true;
            }
        }
    }

    fn handle_match_stmt(&mut self, match_stmt: &'a ast::StmtMatch) {
        let parent_scope = *self.scopes.last().unwrap();
        let mut case_counts = Vec::new();
        for case in &match_stmt.cases {
            self.enter_scope(parent_scope);
            self.visit_match_case(case);
            case_counts.push(self.exit_scope());
        }

        let max_non_return_case_branch = case_counts
            .into_iter()
            .map(|(counts, returns)| if returns { 0 } else { counts })
            .max()
            .unwrap_or(parent_scope.yield_count);

        if let Some(scope) = self.scopes.last_mut() {
            scope.yield_count = max_non_return_case_branch;
        }
    }

    fn handle_try_statement(&mut self, try_stmt: &'a ast::StmtTry) {
        let parent_scope = *self.scopes.last().unwrap();

        self.enter_scope(parent_scope);
        self.visit_body(&try_stmt.body);
        let (try_count, try_returns) = self.exit_scope();

        let normal_path_at_try =
            YieldScope::from(try_count, try_returns, parent_scope.nested_in_loop);

        let mut except_counts = Vec::new();
        for handler in &try_stmt.handlers {
            self.enter_scope(parent_scope);
            self.visit_except_handler(handler);
            except_counts.push(self.exit_scope());
        }

        let normal_at_else = if try_returns {
            // If try returns, else doesn't run
            normal_path_at_try
        } else {
            self.enter_scope(normal_path_at_try);
            self.visit_body(&try_stmt.orelse);
            let (else_count, else_returns) = self.exit_scope();
            YieldScope::from(else_count, else_returns, normal_path_at_try.nested_in_loop)
        };

        // Max returning branches; max non-returning branches
        let (mut continuing, mut returning) = except_counts.into_iter().fold(
            (Vec::new(), Vec::new()),
            |(mut continuing, mut returning), (count, returns)| {
                if returns {
                    returning.push(count);
                } else {
                    continuing.push(count);
                }
                (continuing, returning)
            },
        );

        if normal_at_else.returned {
            returning.push(normal_at_else.yield_count);
        } else {
            continuing.push(normal_at_else.yield_count);
        }

        let n_continuing = continuing.len();

        // Handle finally for terminating and non-terminating control flow branches

        // Terminating branches
        let max_returning = returning.into_iter().max().unwrap_or(0);
        let terminating_finally_scope =
            YieldScope::from(max_returning, true, parent_scope.nested_in_loop);
        self.enter_scope(terminating_finally_scope);
        self.visit_body(&try_stmt.finalbody);
        let _ = self.exit_scope(); // Handle with the continuing branches

        // Continuing brancehs
        let max_continuing = continuing.into_iter().max().unwrap_or(0);
        let maybe_continuing_finally_scope =
            YieldScope::from(max_continuing, false, parent_scope.nested_in_loop);
        self.enter_scope(maybe_continuing_finally_scope);
        self.visit_body(&try_stmt.finalbody);
        let (continuing_finally_count, continuing_finally_returns) = self.exit_scope();

        if let Some(scope) = self.scopes.last_mut() {
            scope.yield_count = continuing_finally_count;
            // Finally returns or all other previous branches return
            scope.returned = continuing_finally_returns || n_continuing == 0;
        }
    }
}

impl<'a> SourceOrderVisitor<'a> for YieldTracker<'a, '_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                let scope = self.scopes.last_mut().unwrap();
                scope.yield_count += 1;
                // If the scope already returned, subsequent yields are unreachable
                if (scope.yield_count > 1 || scope.nested_in_loop) && !scope.returned {
                    self.emit_violation_at_range(expr.range());
                }
            }
            Expr::Lambda(_) | Expr::Generator(_) => {
                // Yields in generators or lambdas don't yield the contextmanager
            }
            _ => source_order::walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &'a ruff_python_ast::Stmt) {
        match stmt {
            ast::Stmt::Return(_) => {
                let scope = self.scopes.last_mut().unwrap();
                scope.returned = true;
            }

            ast::Stmt::If(if_stmt) => {
                self.handle_if_stmt(if_stmt);
            }

            ast::Stmt::For(for_stmt) => {
                self.handle_loop(&for_stmt.body, &for_stmt.orelse);
            }

            ast::Stmt::While(while_stmt) => {
                self.handle_loop(&while_stmt.body, &while_stmt.orelse);
            }

            ast::Stmt::Try(try_stmt) => {
                self.handle_try_statement(try_stmt);
            }

            ast::Stmt::Match(match_stmt) => {
                self.handle_match_stmt(match_stmt);
            }

            ast::Stmt::FunctionDef(nested) => {
                // Don't traverse into nested functions
                function_def_visit_sourceorder_except_body(nested, self);
            }

            _ => source_order::walk_stmt(self, stmt),
        }
    }
}
