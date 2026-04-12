use std::collections::HashMap;
use std::fmt::Debug;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::statement_visitor::{self, StatementVisitor};
use ruff_python_ast::{
    ExceptHandler, ExceptHandlerExceptHandler, Expr, ExprAttribute, ExprCall, ExprSubscript,
    ExprTuple, Stmt, StmtAssign, StmtAugAssign, StmtDelete, StmtFor, StmtIf, StmtTry, StmtWhile,
    visitor::{self, Visitor},
};
use ruff_text_size::TextRange;

use crate::Violation;
use crate::checkers::ast::Checker;
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
#[violation_metadata(preview_since = "v0.3.7")]
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
pub(crate) fn loop_iterator_mutation(checker: &Checker, stmt_for: &StmtFor) {
    let StmtFor {
        target,
        iter,
        body,
        orelse: _,
        is_async: _,
        range: _,
        node_index: _,
    } = stmt_for;

    let (index, target, iter) = match iter.as_ref() {
        Expr::Name(_) | Expr::Attribute(_) => {
            // Ex) Given, `for item in items:`, `item` is the index and `items` is the iterable.
            (&**target, &**target, &**iter)
        }
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            // Ex) Given `for i, item in enumerate(items):`, `i` is the index and `items` is the
            // iterable.
            if checker.semantic().match_builtin_expr(func, "enumerate") {
                // Ex) `items`
                let Some(iter) = arguments.args.first() else {
                    return;
                };

                let Expr::Tuple(ExprTuple { elts, .. }) = &**target else {
                    return;
                };

                let [index, target] = elts.as_slice() else {
                    return;
                };

                // Ex) `i`
                (index, target, iter)
            } else {
                return;
            }
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

/// Whether `body` contains a `break` that would exit the enclosing loop
/// (not one inside a further-nested loop). When absent, the loop's `else`
/// clause is guaranteed to run.
fn body_has_reachable_break(body: &[Stmt]) -> bool {
    let mut finder = BreakFinder::default();
    finder.visit_body(body);
    finder.found
}

#[derive(Default)]
struct BreakFinder {
    found: bool,
}

impl StatementVisitor<'_> for BreakFinder {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        if self.found {
            return;
        }
        match stmt {
            Stmt::Break(_) => self.found = true,
            // Don't look inside nested loop bodies, but do check their
            // `else` clause — a `break` there targets the enclosing loop.
            Stmt::For(StmtFor { orelse, .. }) | Stmt::While(StmtWhile { orelse, .. }) => {
                self.visit_body(orelse);
            }
            // Nested function/class bodies can't affect the enclosing
            // loop's control flow.
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => statement_visitor::walk_stmt(self, stmt),
        }
    }
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
    iter: &'a Expr,
    target: &'a Expr,
    index: &'a Expr,
    mutations: HashMap<u32, Vec<TextRange>>,
    branch: u32,
    next_branch_id: u32,
    loop_depth: u32,
}

impl<'a> LoopMutationsVisitor<'a> {
    /// Initialize the visitor.
    fn new(iter: &'a Expr, target: &'a Expr, index: &'a Expr) -> Self {
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
    fn handle_delete(&mut self, range: TextRange, targets: &[Expr]) {
        for target in targets {
            if let Expr::Subscript(ExprSubscript {
                range: _,
                node_index: _,
                value,
                slice: _,
                ctx: _,
            }) = target
            {
                // Find, e.g., `del items[0]`.
                if ComparableExpr::from(self.iter) == ComparableExpr::from(value) {
                    self.add_mutation(range);
                }
            }
        }
    }

    /// Handle, e.g., `items[0] = 1`.
    fn handle_assign(&mut self, range: TextRange, targets: &[Expr]) {
        for target in targets {
            if let Expr::Subscript(ExprSubscript {
                range: _,
                node_index: _,
                value,
                slice,
                ctx: _,
            }) = target
            {
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
    fn handle_aug_assign(&mut self, range: TextRange, target: &Expr) {
        if ComparableExpr::from(self.iter) == ComparableExpr::from(target) {
            self.add_mutation(range);
        }
    }

    /// Handle, e.g., `items.append(1)`.
    fn handle_call(&mut self, func: &Expr) {
        if let Expr::Attribute(ExprAttribute {
            range,
            node_index: _,
            value,
            attr,
            ctx: _,
        }) = func
        {
            if is_mutating_function(attr.as_str()) {
                // Find, e.g., `items.remove(1)`.
                if ComparableExpr::from(self.iter) == ComparableExpr::from(value) {
                    self.add_mutation(*range);
                }
            }
        }
    }
}

/// Walk statements to detect mutations and track control-flow terminators.
impl<'a> Visitor<'a> for LoopMutationsVisitor<'a> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        for stmt in body {
            self.visit_stmt(stmt);
            // After a terminator, remaining statements are unreachable.
            if matches!(stmt, Stmt::Break(_) | Stmt::Return(_) | Stmt::Continue(_)) {
                break;
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // Ex) `del items[0]`
            Stmt::Delete(StmtDelete {
                range,
                targets,
                node_index: _,
            }) => {
                self.handle_delete(*range, targets);
                visitor::walk_stmt(self, stmt);
            }

            // Ex) `items[0] = 1`
            Stmt::Assign(StmtAssign { range, targets, .. }) => {
                self.handle_assign(*range, targets);
                visitor::walk_stmt(self, stmt);
            }

            // Ex) `items += [1]`
            Stmt::AugAssign(StmtAugAssign { range, target, .. }) => {
                self.handle_aug_assign(*range, target);
                visitor::walk_stmt(self, stmt);
            }

            // Ex) `for y in other: ...`
            Stmt::For(StmtFor {
                target,
                iter,
                body,
                orelse,
                ..
            }) => {
                self.visit_expr(iter);
                self.visit_expr(target);

                let saved_branch = self.branch;

                self.enter_new_branch();
                self.loop_depth += 1;
                self.visit_body(body);
                self.loop_depth -= 1;
                self.merge_branch_into(saved_branch);

                // If the body never breaks, the else clause always runs,
                // so a terminator there can clear earlier mutations.
                if !orelse.is_empty() {
                    if body_has_reachable_break(body) {
                        self.enter_new_branch();
                        self.visit_body(orelse);
                        self.merge_branch_into(saved_branch);
                    } else {
                        self.visit_body(orelse);
                    }
                }
            }

            // Ex) `while cond: ...`
            Stmt::While(StmtWhile {
                test, body, orelse, ..
            }) => {
                self.visit_expr(test);

                let saved_branch = self.branch;

                self.enter_new_branch();
                self.loop_depth += 1;
                self.visit_body(body);
                self.loop_depth -= 1;
                self.merge_branch_into(saved_branch);

                if !orelse.is_empty() {
                    if body_has_reachable_break(body) {
                        self.enter_new_branch();
                        self.visit_body(orelse);
                        self.merge_branch_into(saved_branch);
                    } else {
                        self.visit_body(orelse);
                    }
                }
            }

            // Ex) `if True: items.append(1)`
            Stmt::If(StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => {
                let saved_branch = self.branch;

                // Handle the `if` branch.
                self.enter_new_branch();
                self.visit_expr(test);
                self.visit_body(body);
                self.merge_branch_into(saved_branch);

                // Handle the `elif` and `else` branches.
                for clause in elif_else_clauses {
                    self.enter_new_branch();
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                    }
                    self.visit_body(&clause.body);
                    self.merge_branch_into(saved_branch);
                }
            }

            // Ex) `try: ... except: ... else: ... finally: ...`
            Stmt::Try(StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
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
                    let ExceptHandler::ExceptHandler(ExceptHandlerExceptHandler {
                        type_,
                        body,
                        ..
                    }) = handler;
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
                }
            }

            // Return exits the function; the loop can't re-iterate.
            Stmt::Return(_) => {
                if let Some(mutations) = self.mutations.get_mut(&self.branch) {
                    mutations.clear();
                }
            }

            // Only clear at the outermost loop — a nested break doesn't
            // stop the outer iteration.
            Stmt::Break(_) => {
                if self.loop_depth == 0
                    && let Some(mutations) = self.mutations.get_mut(&self.branch)
                {
                    mutations.clear();
                }
            }

            // Mutation still reachable on the next iteration.
            Stmt::Continue(_) => {}

            // Avoid recursion for class and function definitions.
            Stmt::ClassDef(_) | Stmt::FunctionDef(_) => {}

            // Default case.
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        // Ex) `items.append(1)`
        if let Expr::Call(ExprCall { func, .. }) = expr {
            self.handle_call(func);
        }

        visitor::walk_expr(self, expr);
    }
}
