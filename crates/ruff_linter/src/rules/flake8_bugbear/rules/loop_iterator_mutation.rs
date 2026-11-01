use ruff_text_size::Ranged;
use std::collections::HashMap;
use std::fmt::Debug;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    self as ast, Expr, ExprKind, NodeList, Stmt, StmtFor, StmtKind, Suite, statement_visitor,
    visitor,
};
use ruff_text_size::TextRange;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for mutations to an iterable during a loop iteration.
///
/// ## Why is this bad?
/// When iterating over an iterable, mutating the iterable can lead to unexpected
/// behavior, like skipping elements or infinite loops.
///
/// ## Example
/// ```python
/// items = [1, 2, 3]
///
/// for item in items:
///     print(item)
///
///     # Create an infinite loop by appending to the list.
///     items.append(item)
/// ```
///
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html#typesseq-mutable)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.3.7", category = Category::Suspicious)]
pub(crate) struct LoopIteratorMutation {
    name: Option<SourceCodeSnippet>,
}

impl Violation for LoopIteratorMutation {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(name) = self.name.as_ref().and_then(SourceCodeSnippet::full_display) {
            format!("Mutation to loop iterable `{name}` during iteration")
        } else {
            "Mutation to loop iterable during iteration".to_string()
        }
    }
}

/// B909
pub(crate) fn loop_iterator_mutation(checker: &Checker, stmt_for: StmtFor<'_>) {
    let target = stmt_for.target();
    let iter = stmt_for.iter();
    let body = stmt_for.body();

    let (index, target, iter) = match iter.classify() {
        ExprKind::Name(_) | ExprKind::Attribute(_) => {
            // Ex) Given, `for item in items:`, `item` is the index and `items` is the iterable.
            (target, target, iter)
        }
        // Ex) Given `for i, item in enumerate(items):`, `i` is the index and `items` is the
        // iterable.
        ExprKind::Call(expr_call)
            if checker
                .semantic()
                .match_builtin_expr(expr_call.func(), "enumerate") =>
        {
            let arguments = expr_call.arguments();

            // Ex) `items`
            let Some(iter) = arguments.args().first() else {
                return;
            };

            let ExprKind::Tuple(tuple) = target.classify() else {
                return;
            };
            let elts = tuple.elts();

            let Some([index, target]) = elts.as_array() else {
                return;
            };

            // Ex) `i`
            (index, target, iter)
        }
        _ => {
            return;
        }
    };

    // Collect mutations to the iterable.
    let mutations = {
        let mut visitor = LoopMutationsVisitor::new(iter, target, index);
        visitor.visit_body(body);
        visitor.mutations
    };

    // Create a diagnostic for each mutation.
    for mutation in mutations.values().flatten() {
        let name = UnqualifiedName::from_expr(iter)
            .map(|name| name.to_string())
            .map(SourceCodeSnippet::new);
        checker.report_diagnostic(LoopIteratorMutation { name }, *mutation);
    }
}

/// Whether `body` contains a `break`, `return`, or `raise` that would cause
/// the enclosing loop's `else` clause to be skipped.
///
/// See [the Python tutorial on `else` clauses on loops][else-clauses].
///
/// [else-clauses]: https://docs.python.org/3/tutorial/controlflow.html#else-clauses-on-loops
fn body_may_skip_else(body: Suite<'_>) -> bool {
    let mut finder = SkipsElseFinder::default();
    finder.visit_body(body);
    finder.found
}

#[derive(Default)]
struct SkipsElseFinder {
    found: bool,
}

impl StatementVisitor<'_> for SkipsElseFinder {
    fn visit_body(&mut self, body: Suite<'_>) {
        for stmt in body {
            self.visit_stmt(stmt);
            // After a terminator, remaining statements are unreachable.
            if matches!(
                stmt.classify(),
                StmtKind::Break(_)
                    | StmtKind::Return(_)
                    | StmtKind::Continue(_)
                    | StmtKind::Raise(_)
            ) {
                break;
            }
        }
    }

    fn visit_stmt(&mut self, stmt: Stmt<'_>) {
        if self.found {
            return;
        }
        match stmt.fields() {
            // `break`/`return`/`raise` all skip the enclosing loop's
            // `else` clause; see Python docs on `else` clauses for loops.
            ast::StmtFields::Break(_) | ast::StmtFields::Return(_) | ast::StmtFields::Raise(_) => {
                self.found = true;
            }
            // Don't look inside nested loop bodies, but do check their
            // `else` clause — a `break` there targets the enclosing loop.
            ast::StmtFields::For(ast::StmtForFields { orelse, .. })
            | ast::StmtFields::While(ast::StmtWhileFields { orelse, .. }) => {
                self.visit_body(orelse);
            }
            // Nested function/class bodies can't affect the enclosing
            // loop's control flow.
            ast::StmtFields::FunctionDef(_) | ast::StmtFields::ClassDef(_) => {}
            _ => statement_visitor::walk_stmt(self, stmt),
        }
    }
}

/// Whether `body` has an unconditional terminator at its top level.
/// Used to decide if a `try` statement's `finally` clause always exits the
/// enclosing control flow (loop or function).
fn unconditionally_terminates(body: Suite<'_>) -> bool {
    body.iter().any(|stmt| {
        matches!(
            stmt.classify(),
            StmtKind::Return(_) | StmtKind::Raise(_) | StmtKind::Break(_)
        )
    })
}

/// Returns `true` if the method mutates when called on an iterator.
fn is_mutating_function(function_name: &str) -> bool {
    matches!(
        function_name,
        "append"
            | "sort"
            | "reverse"
            | "remove"
            | "clear"
            | "extend"
            | "insert"
            | "pop"
            | "popitem"
            | "setdefault"
            | "update"
            | "intersection_update"
            | "difference_update"
            | "symmetric_difference_update"
            | "add"
            | "discard"
    )
}

/// Collects mutations to a loop iterable, accounting for control flow.
///
/// Mutations in different arms of `if`/`try`/`for`/`while` are tracked in
/// separate branches so a terminator in one arm can't clear a sibling's
/// mutations. On arm exit, mutations merge back into the enclosing branch.
///
/// `loop_depth` prevents a nested loop's `break` from clearing the outer
/// loop's mutations.
#[derive(Debug)]
struct LoopMutationsVisitor<'a> {
    iter: Expr<'a>,
    target: Expr<'a>,
    index: Expr<'a>,
    mutations: HashMap<u32, Vec<TextRange>>,
    branch: u32,
    next_branch_id: u32,
    loop_depth: u32,
}

impl<'a> LoopMutationsVisitor<'a> {
    /// Initialize the visitor.
    fn new(iter: Expr<'a>, target: Expr<'a>, index: Expr<'a>) -> Self {
        Self {
            iter,
            target,
            index,
            mutations: HashMap::new(),
            branch: 0,
            next_branch_id: 0,
            loop_depth: 0,
        }
    }

    /// Allocate a fresh branch ID and make it current.
    fn enter_new_branch(&mut self) {
        self.next_branch_id += 1;
        self.branch = self.next_branch_id;
    }

    /// Merge the current branch's mutations into `parent` and switch to it.
    fn merge_branch_into(&mut self, parent: u32) {
        if let Some(child_mutations) = self.mutations.remove(&self.branch) {
            self.mutations
                .entry(parent)
                .or_default()
                .extend(child_mutations);
        }
        self.branch = parent;
    }

    /// Register a mutation.
    fn add_mutation(&mut self, range: TextRange) {
        self.mutations.entry(self.branch).or_default().push(range);
    }

    /// Handle, e.g., `del items[0]`.
    fn handle_delete(
        &mut self,
        range: TextRange,
        targets: NodeList<'_, ruff_python_ast::kind::Expr>,
    ) {
        for target in targets {
            if let ExprKind::Subscript(subscript) = target.classify() {
                let value = subscript.value();
                // Find, e.g., `del items[0]`.
                if ComparableExpr::from(self.iter) == ComparableExpr::from(value) {
                    self.add_mutation(range);
                }
            }
        }
    }

    /// Handle, e.g., `items[0] = 1`.
    fn handle_assign(
        &mut self,
        range: TextRange,
        targets: NodeList<'_, ruff_python_ast::kind::Expr>,
    ) {
        for target in targets {
            if let ExprKind::Subscript(subscript) = target.classify() {
                let value = subscript.value();
                let slice = subscript.slice();
                // Find, e.g., `items[0] = 1`.
                if ComparableExpr::from(self.iter) == ComparableExpr::from(value) {
                    // But allow, e.g., `for item in items: items[item] = 1`.
                    if ComparableExpr::from(self.index) != ComparableExpr::from(slice)
                        && ComparableExpr::from(self.target) != ComparableExpr::from(slice)
                    {
                        self.add_mutation(range);
                    }
                }
            }
        }
    }

    /// Handle, e.g., `items += [1]`.
    fn handle_aug_assign(&mut self, range: TextRange, target: Expr<'_>) {
        if ComparableExpr::from(self.iter) == ComparableExpr::from(target) {
            self.add_mutation(range);
        }
    }

    /// Handle, e.g., `items.append(1)`.
    fn handle_call(&mut self, func: Expr<'_>) {
        if let ExprKind::Attribute(attribute) = func.classify() {
            let range = attribute.range();
            let value = attribute.value();
            let attr = attribute.attr();
            if is_mutating_function(attr.as_str()) {
                // Find, e.g., `items.remove(1)`.
                if ComparableExpr::from(self.iter) == ComparableExpr::from(value) {
                    self.add_mutation(range);
                }
            }
        }
    }
}

/// Walk statements to detect mutations and track control-flow terminators.
impl<'a> Visitor<'a> for LoopMutationsVisitor<'a> {
    fn visit_body(&mut self, body: Suite<'a>) {
        for stmt in body {
            self.visit_stmt(stmt);
            // After a terminator, remaining statements are unreachable.
            if matches!(
                stmt.classify(),
                StmtKind::Break(_)
                    | StmtKind::Return(_)
                    | StmtKind::Continue(_)
                    | StmtKind::Raise(_)
            ) {
                break;
            }
        }
    }

    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        match stmt.classify() {
            // Ex) `del items[0]`
            StmtKind::Delete(stmt_delete) => {
                let range = stmt_delete.range();
                let targets = stmt_delete.targets();

                self.handle_delete(range, targets);
                visitor::walk_stmt(self, stmt);
            }

            // Ex) `items[0] = 1`
            StmtKind::Assign(stmt_assign) => {
                let range = stmt_assign.range();
                let targets = stmt_assign.targets();

                self.handle_assign(range, targets);
                visitor::walk_stmt(self, stmt);
            }

            // Ex) `items += [1]`
            StmtKind::AugAssign(stmt_aug_assign) => {
                let range = stmt_aug_assign.range();
                let target = stmt_aug_assign.target();

                self.handle_aug_assign(range, target);
                visitor::walk_stmt(self, stmt);
            }

            // Ex) `for y in other: ...`
            StmtKind::For(stmt_for) => {
                let target = stmt_for.target();
                let iter = stmt_for.iter();
                let body = stmt_for.body();
                let orelse = stmt_for.orelse();

                self.visit_expr(iter);
                self.visit_expr(target);

                let saved_branch = self.branch;

                self.enter_new_branch();
                self.loop_depth += 1;
                self.visit_body(body);
                self.loop_depth -= 1;
                self.merge_branch_into(saved_branch);

                // If the body may `break`, `return`, or `raise`, the `else`
                // clause may not run, so its terminators must not clear
                // mutations from the body.
                if !orelse.is_empty() {
                    if body_may_skip_else(body) {
                        self.enter_new_branch();
                        self.visit_body(orelse);
                        self.merge_branch_into(saved_branch);
                    } else {
                        self.visit_body(orelse);
                    }
                }
            }

            // Ex) `while cond: ...`
            StmtKind::While(stmt_while) => {
                let test = stmt_while.test();
                let body = stmt_while.body();
                let orelse = stmt_while.orelse();

                self.visit_expr(test);

                let saved_branch = self.branch;

                self.enter_new_branch();
                self.loop_depth += 1;
                self.visit_body(body);
                self.loop_depth -= 1;
                self.merge_branch_into(saved_branch);

                if !orelse.is_empty() {
                    if body_may_skip_else(body) {
                        self.enter_new_branch();
                        self.visit_body(orelse);
                        self.merge_branch_into(saved_branch);
                    } else {
                        self.visit_body(orelse);
                    }
                }
            }

            // Ex) `if True: items.append(1)`
            StmtKind::If(stmt_if) => {
                let test = stmt_if.test();
                let body = stmt_if.body();
                let elif_else_clauses = stmt_if.elif_else_clauses();

                let saved_branch = self.branch;

                // Handle the `if` branch.
                self.enter_new_branch();
                self.visit_expr(test);
                self.visit_body(body);
                self.merge_branch_into(saved_branch);

                // Handle the `elif` and `else` branches.
                for clause in elif_else_clauses {
                    self.enter_new_branch();
                    self.visit_elif_else_clause(clause);
                    self.merge_branch_into(saved_branch);
                }
            }

            // Ex) `try: ... except: ... else: ... finally: ...`
            StmtKind::Try(stmt_try) => {
                let body = stmt_try.body();
                let handlers = stmt_try.handlers();
                let orelse = stmt_try.orelse();
                let finalbody = stmt_try.finalbody();

                let saved_branch = self.branch;

                self.enter_new_branch();
                self.visit_body(body);
                self.merge_branch_into(saved_branch);

                if !orelse.is_empty() {
                    self.enter_new_branch();
                    self.visit_body(orelse);
                    self.merge_branch_into(saved_branch);
                }

                for handler in handlers {
                    let ast::ExceptHandlerKind::ExceptHandler(handler) = handler.classify();
                    let type_ = handler.type_();
                    let body = handler.body();
                    self.enter_new_branch();
                    if let Some(type_) = type_ {
                        self.visit_expr(type_);
                    }
                    self.visit_body(body);
                    self.merge_branch_into(saved_branch);
                }

                // Give `finally` its own branch so siblings don't
                // cross-clear through it.
                if !finalbody.is_empty() {
                    self.enter_new_branch();
                    self.visit_body(finalbody);
                    self.merge_branch_into(saved_branch);

                    // If `finally` unconditionally terminates (e.g., ends in
                    // `return`), the entire `try` statement always exits the
                    // enclosing control flow, so earlier mutations never
                    // reach another iteration.
                    if unconditionally_terminates(finalbody)
                        && let Some(mutations) = self.mutations.get_mut(&saved_branch)
                    {
                        mutations.clear();
                    }
                }
            }

            // Return exits the function; the loop can't re-iterate.
            StmtKind::Return(_) => {
                if let Some(mutations) = self.mutations.get_mut(&self.branch) {
                    mutations.clear();
                }
            }

            // Only clear at the outermost loop — a nested break doesn't
            // stop the outer iteration.
            StmtKind::Break(_) => {
                if self.loop_depth == 0
                    && let Some(mutations) = self.mutations.get_mut(&self.branch)
                {
                    mutations.clear();
                }
            }

            // Mutation still reachable on the next iteration.
            StmtKind::Continue(_) => {}

            // Avoid recursion for class and function definitions.
            StmtKind::ClassDef(_) | StmtKind::FunctionDef(_) => {}

            // Default case.
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: Expr<'a>) {
        // Ex) `items.append(1)`
        if let ExprKind::Call(call) = expr.classify() {
            let func = call.func();
            self.handle_call(func);
        }

        visitor::walk_expr(self, expr);
    }
}
