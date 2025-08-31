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

// YieldTracker tracks yield expressions along the control flow path.
// If we encounter multiple yields in a single path, the contextmanager protocol is broken
// and we collect violations to be emitted later.
//
// The tracker maintains a stack of scopes that contain the scope yield expressions
// and whether the scope returns (to determine if we need to continue traversing the path).
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

        // Update current scope to reflect the maximum path
        if let Some(scope) = self.scopes.last_mut() {
            if continuing.is_empty() {
                // If unconditional branch exists and all return, no subsequent
                // code reached
                scope.returned = has_else;
            } else {
                scope.yield_count = *continuing.iter().max().unwrap();
            }
        }
    }

    fn handle_loop(&mut self, body: &'a [ast::Stmt], orelse: &'a [ast::Stmt]) {
        let mut parent_scope = *self.scopes.last().unwrap();
        parent_scope.nested_in_loop = true;

        self.enter_scope(parent_scope);
        self.visit_body(body);
        let _ = self.exit_scope(); // Ignore - violations already reported

        // Else clause only runs if loop doesn't break
        let else_parent = *self.scopes.last().unwrap();
        self.enter_scope(else_parent);
        self.visit_body(orelse);
        let (else_count, else_returns) = self.exit_scope();

        if let Some(scope) = self.scopes.last_mut() {
            scope.yield_count += else_count;
            if else_returns {
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
        // Try -> Else -> Finally
        // Try -> Except(s) -> Finally
        // Each should check their own yield counts.
        // We propagate the max yield count along a non-return path
        //

        self.enter_scope(parent_scope);
        self.visit_body(&try_stmt.body);
        let (try_count, try_returns) = self.exit_scope();

        let normal_at_try = YieldScope::from(try_count, try_returns, parent_scope.nested_in_loop);

        let mut except_counts = Vec::new();
        for handler in &try_stmt.handlers {
            self.enter_scope(parent_scope);
            self.visit_except_handler(handler);
            except_counts.push(self.exit_scope());
        }

        let normal_at_else = if try_returns {
            // If try returns, else doesn't run
            normal_at_try
        } else {
            self.enter_scope(normal_at_try);
            self.visit_body(&try_stmt.orelse);
            let (else_count, else_returns) = self.exit_scope();
            YieldScope::from(else_count, else_returns, normal_at_try.nested_in_loop)
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

        // Handle finally that terminates because of previous return
        let max_returning = returning.into_iter().max().unwrap_or(0);
        let terminating_finally_scope =
            YieldScope::from(max_returning, true, parent_scope.nested_in_loop);
        self.enter_scope(terminating_finally_scope);
        self.visit_body(&try_stmt.finalbody);
        let _ = self.exit_scope(); // Handle with the continuing branches

        // Handle finally that did not yet encounter return
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

//
//
// struct YieldTracker<'a, 'b> {
//     checker: &'a Checker<'b>,
//     name: &'static str,
//     scopes: Vec<YieldScope<'a>>,
//     reported_ranges: FxHashSet<TextRange>,
// }
//
// impl<'a, 'b> YieldTracker<'a, 'b> {
//     fn new(checker: &'a Checker<'b>, name: &'static str) -> Self {
//         Self {
//             checker,
//             name,
//             scopes: vec![YieldScope::new()],
//             reported_ranges: FxHashSet::default(),
//         }
//     }
//
//     fn add_yield(&mut self, expr: &'a Expr) {
//         if let Some(scope) = self.scopes.last_mut() {
//             scope.yield_expressions.push(expr);
//             if scope.yields_excessively() {
//                 self.emit_violation(expr);
//             }
//         }
//     }
//
//     fn emit_violation(&mut self, expr: &Expr) {
//         let range = expr.range();
//         if self.reported_ranges.insert(range) {
//             self.checker
//                 .report_diagnostic(MultipleYieldsInContextManager::new(self.name), range);
//         }
//     }
//
//     fn emit_multiple_violations<'e>(&mut self, yields: impl IntoIterator<Item = &'e &'e Expr>) {
//         // The first yield conforms to the protocol
//         for yield_expr in yields.into_iter().skip(1) {
//             self.emit_violation(yield_expr);
//         }
//     }
//
//     fn propagate_yields(&mut self, yields: &[&'a Expr]) {
//         for &yield_expr in yields {
//             self.add_yield(yield_expr);
//         }
//     }
//
//     fn push_scope(&mut self) {
//         self.scopes.push(YieldScope::new());
//     }
//
//     fn pop_scope(&mut self) -> Option<(Vec<&'a Expr>, bool)> {
//         self.scopes
//             .pop()
//             .map(|scope| (scope.yield_expressions, scope.does_return))
//     }
//
//     fn max_yields(branches: &[Vec<&'a Expr>]) -> Vec<&'a Expr> {
//         branches
//             .iter()
//             .max_by_key(|branch| branch.len())
//             .cloned()
//             .unwrap_or_default()
//     }
//
//     fn join_paths(base: &[&'a Expr], finally_yields: &[&'a Expr]) -> Vec<&'a Expr> {
//         let mut path = base.to_vec();
//         path.extend_from_slice(finally_yields);
//         path
//     }
//
//     fn handle_loop(&mut self, body: &'a [ast::Stmt], orelse: &'a [ast::Stmt]) {
//         self.visit_body(body);
//         self.push_scope();
//         self.visit_body(orelse);
//     }
//
//     fn handle_try_statement(&mut self, try_stmt: &ast::StmtTry) {
//         let (finally_yields, finally_returns) = self
//             .pop_scope()
//             .expect("Missing finally block scope in try-statement");
//
//         let (else_yields, else_returns) = self
//             .pop_scope()
//             .expect("Missing else block scope in try-statement");
//
//         let mut returning_except_branches = Vec::new();
//         let mut continuing_except_branches = Vec::new();
//
//         for _ in 0..try_stmt.handlers.len() {
//             let (except_yields, except_returns) = self
//                 .pop_scope()
//                 .expect("Missing except handler scope in try-statement");
//             self.emit_multiple_violations(&except_yields);
//
//             if except_returns {
//                 returning_except_branches.push(except_yields);
//             } else {
//                 continuing_except_branches.push(except_yields);
//             }
//         }
//
//         let (try_yields, try_returns) = self
//             .pop_scope()
//             .expect("Missing try block scope in try-statement");
//
//         let path = TryExceptPath {
//             try_yields,
//             try_returns,
//             else_yields,
//             else_returns,
//             finally_yields,
//             returning_except_branches,
//             continuing_except_branches,
//         };
//
//         if finally_returns {
//             self.handle_terminating_paths(&path);
//         } else {
//             self.handle_continuing_paths(&path);
//         }
//     }
//
//     // Finally returns - execution stops, report worst-case path
//     fn handle_terminating_paths(&mut self, path: &TryExceptPath<'a>) {
//         let except_yields = Self::get_max_except_path(path);
//         let base_path = Self::build_pre_finally_path(path, &except_yields);
//         let max_path = Self::join_paths(&base_path, &path.finally_yields);
//
//         self.emit_multiple_violations(&max_path);
//     }
//
//     // Finally doesn't return - execution continues, handle all paths
//     fn handle_continuing_paths(&mut self, path: &TryExceptPath<'a>) {
//         let (exception_return, exception_no_return) = Self::build_except_paths(path);
//
//         self.emit_multiple_violations(&exception_return);
//
//         let normal_path = Self::build_try_else_path(path);
//
//         self.propagate_continuing_paths(path, &normal_path, &exception_no_return);
//     }
//
//     fn handle_exclusive_branches(&mut self, branch_count: usize) {
//         let mut continuing_branches = Vec::new();
//
//         for _ in 0..branch_count {
//             let (branch_yields, branch_returns) = self
//                 .pop_scope()
//                 .expect("Missing branch scope in if/match statement");
//             self.emit_multiple_violations(&branch_yields);
//
//             if !branch_returns {
//                 continuing_branches.push(branch_yields);
//             }
//         }
//
//         let max_continuing = Self::max_yields(&continuing_branches);
//
//         self.propagate_yields(&max_continuing);
//     }
//
//     // Path building methods
//     // Find except handler with most yields
//     fn get_max_except_path(path: &TryExceptPath<'a>) -> Vec<&'a Expr> {
//         let max_returning_except = Self::max_yields(&path.returning_except_branches);
//         let max_continuing_except = Self::max_yields(&path.continuing_except_branches);
//
//         if max_returning_except.len() > max_continuing_except.len() {
//             max_returning_except
//         } else {
//             max_continuing_except
//         }
//     }
//
//     // Build path before finally: try + (else or except)
//     fn build_pre_finally_path(
//         path: &TryExceptPath<'a>,
//         max_except_yields: &[&'a Expr],
//     ) -> Vec<&'a Expr> {
//         let mut common_path = path.try_yields.clone();
//
//         if !path.try_returns {
//             common_path.extend(if path.else_yields.len() > max_except_yields.len() {
//                 &path.else_yields
//             } else {
//                 max_except_yields
//             });
//         }
//
//         common_path
//     }
//
//     // Build exception paths with finally appended
//     fn build_except_paths(path: &TryExceptPath<'a>) -> (Vec<&'a Expr>, Vec<&'a Expr>) {
//         let max_returning_except = Self::max_yields(&path.returning_except_branches);
//         let max_continuing_except = Self::max_yields(&path.continuing_except_branches);
//
//         let exception_return = Self::join_paths(&max_returning_except, &path.finally_yields);
//         let exception_no_return = Self::join_paths(&max_continuing_except, &path.finally_yields);
//
//         (exception_return, exception_no_return)
//     }
//
//     // Build normal execution path: try + else + finally
//     fn build_try_else_path(path: &TryExceptPath<'a>) -> Vec<&'a Expr> {
//         let mut try_else = path.try_yields.clone();
//         try_else.extend(&path.else_yields);
//         Self::join_paths(&try_else, &path.finally_yields)
//     }
//
//     // Decide which paths propagate based on return statements
//     fn propagate_continuing_paths(
//         &mut self,
//         path: &TryExceptPath<'a>,
//         normal_path: &[&'a Expr],
//         exception_no_return: &[&'a Expr],
//     ) {
//         if path.try_returns {
//             let try_path = Self::join_paths(&path.try_yields, &path.finally_yields);
//             self.emit_multiple_violations(&try_path);
//             self.propagate_yields(exception_no_return);
//         } else if path.else_returns {
//             self.emit_multiple_violations(normal_path);
//             self.propagate_yields(exception_no_return);
//         } else {
//             let max_yield_path = if normal_path.len() > exception_no_return.len() {
//                 normal_path.to_vec()
//             } else {
//                 exception_no_return.to_vec()
//             };
//             self.propagate_yields(&max_yield_path);
//         }
//     }
// }
//
// struct TryExceptPath<'a> {
//     try_yields: Vec<&'a Expr>,
//     try_returns: bool,
//     else_yields: Vec<&'a Expr>,
//     else_returns: bool,
//     finally_yields: Vec<&'a Expr>,
//     returning_except_branches: Vec<Vec<&'a Expr>>,
//     continuing_except_branches: Vec<Vec<&'a Expr>>,
// }
//
// struct YieldScope<'a> {
//     yield_expressions: Vec<&'a Expr>,
//     does_return: bool,
// }
//
// impl<'a> YieldScope<'a> {
//     fn new() -> Self {
//         Self {
//             yield_expressions: Vec::new(),
//             does_return: false,
//         }
//     }
//
//     fn yields_excessively(&self) -> bool {
//         self.yield_expressions.len() > 1
//     }
// }
//
// impl<'a, 'b> source_order::SourceOrderVisitor<'a> for YieldTracker<'a, 'b> {
//     fn enter_node(&mut self, node: AnyNodeRef<'a>) -> source_order::TraversalSignal {
//         match node {
//             AnyNodeRef::StmtFor(_)
//             | AnyNodeRef::StmtWhile(_)
//             | AnyNodeRef::StmtTry(_)
//             | AnyNodeRef::StmtIf(_)
//             | AnyNodeRef::StmtMatch(_)
//             | AnyNodeRef::MatchCase(_) => {
//                 // Track for primary control flow structures
//                 // Optional branches like else/finally clauses are handled in leave_node
//                 // Except is handled in leave node to maintain logical locality
//                 self.push_scope();
//             }
//             _ => {}
//         }
//         source_order::TraversalSignal::Traverse
//     }
//
//     fn leave_node(&mut self, node: AnyNodeRef<'a>) {
//         match node {
//             AnyNodeRef::StmtTry(try_stmt) => {
//                 self.handle_try_statement(try_stmt);
//             }
//             AnyNodeRef::StmtIf(if_stmt) => {
//                 let branch_count = 1 + if_stmt.elif_else_clauses.len();
//                 self.handle_exclusive_branches(branch_count);
//             }
//             AnyNodeRef::StmtMatch(match_stmt) => {
//                 let branch_count = match_stmt.cases.len();
//                 self.handle_exclusive_branches(branch_count);
//             }
//             AnyNodeRef::StmtFor(_) | AnyNodeRef::StmtWhile(_) => {
//                 let (else_yields, else_returns) =
//                     self.pop_scope().expect("Missing loop else scope");
//                 let (body_yields, _body_returns) =
//                     self.pop_scope().expect("Missing loop body scope");
//
//                 if !body_yields.is_empty() {
//                     if body_yields.len() == 1 {
//                         self.emit_violation(body_yields.first().unwrap());
//                     } else {
//                         self.emit_multiple_violations(&body_yields);
//                     }
//                 }
//                 self.propagate_yields(&else_yields);
//                 if else_returns {
//                     // If the loop exits irregularly (break) else isn't executed
//                     // Subsequent yields may be valid
// // We should not count return guarded yields in else
//                     self.scopes.last_mut().unwrap().yield_expressions.clear();
//                 }
//             }
//             _ => {}
//         }
//     }
//
//     fn visit_expr(&mut self, expr: &'a Expr) {
//         match expr {
//             Expr::Yield(_) | Expr::YieldFrom(_) => {
//                 self.add_yield(expr);
//             }
//             _ => source_order::walk_expr(self, expr),
//         }
//     }
//
//     fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
//         match stmt {
//             ast::Stmt::Return(_) => {
//                 if let Some(scope) = self.scopes.last_mut() {
//                     scope.does_return = true;
//                 }
//             }
//             ast::Stmt::FunctionDef(nested) => {
//                 function_def_visit_sourceorder_except_body(nested, self);
//             }
//             ast::Stmt::While(loop_stmt @ ast::StmtWhile { body, orelse, .. }) => {
//                 let node = ruff_python_ast::AnyNodeRef::StmtWhile(loop_stmt);
//                 if self.enter_node(node).is_traverse() {
//                     self.handle_loop(body, orelse);
//                     self.leave_node(node);
//                 }
//             }
//             ast::Stmt::For(loop_stmt @ ast::StmtFor { body, orelse, .. }) => {
//                 let node = ruff_python_ast::AnyNodeRef::StmtFor(loop_stmt);
//                 if self.enter_node(node).is_traverse() {
//                     self.handle_loop(body, orelse);
//                     self.leave_node(node);
//                 }
//             }
//             ast::Stmt::If(
//                 if_stmt @ ast::StmtIf {
//                     body,
//                     elif_else_clauses,
//                     ..
//                 },
//             ) => {
//                 let node = ruff_python_ast::AnyNodeRef::StmtIf(if_stmt);
//                 if self.enter_node(node).is_traverse() {
//                     self.visit_body(body);
//                     for clause in elif_else_clauses {
//                         self.push_scope();
//                         self.visit_elif_else_clause(clause);
//                     }
//                     self.leave_node(node);
//                 }
//             }
//             ast::Stmt::Try(
//                 try_stmt @ ast::StmtTry {
//                     body,
//                     handlers,
//                     orelse,
//                     finalbody,
//                     ..
//                 },
//             ) => {
//                 let node = ruff_python_ast::AnyNodeRef::StmtTry(try_stmt);
//                 if self.enter_node(node).is_traverse() {
//                     self.visit_body(body);
//                     for handler in handlers {
//                         self.push_scope();
//                         self.visit_except_handler(handler);
//                     }
//
//                     self.push_scope();
//                     self.visit_body(orelse);
//                     self.push_scope();
//                     self.visit_body(finalbody);
//                     self.leave_node(node);
//                 }
//             }
//             _ => source_order::walk_stmt(self, stmt),
//         }
//     }
// }
