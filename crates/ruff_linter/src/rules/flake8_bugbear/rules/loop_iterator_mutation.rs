use std::collections::HashMap;
use std::fmt::Debug;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{
    visitor::{self, Visitor},
    Expr, ExprAttribute, ExprCall, ExprSubscript, ExprTuple, Stmt, StmtAssign, StmtAugAssign,
    StmtDelete, StmtFor, StmtIf,
};
use ruff_text_size::TextRange;

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
#[violation]
pub struct LoopIteratorMutation {
    name: Option<SourceCodeSnippet>,
}

impl Violation for LoopIteratorMutation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LoopIteratorMutation { name } = self;

        if let Some(name) = name.as_ref().and_then(SourceCodeSnippet::full_display) {
            format!("Mutation to loop iterable `{name}` during iteration")
        } else {
            format!("Mutation to loop iterable during iteration")
        }
    }
}

/// B909
pub(crate) fn loop_iterator_mutation(checker: &mut Checker, stmt_for: &StmtFor) {
    let StmtFor {
        target,
        iter,
        body,
        orelse: _,
        is_async: _,
        range: _,
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
        checker
            .diagnostics
            .push(Diagnostic::new(LoopIteratorMutation { name }, *mutation));
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

/// A visitor to collect mutations to a variable in a loop.
#[derive(Debug, Clone)]
struct LoopMutationsVisitor<'a> {
    iter: &'a Expr,
    target: &'a Expr,
    index: &'a Expr,
    mutations: HashMap<u32, Vec<TextRange>>,
    branches: Vec<u32>,
    branch: u32,
}

impl<'a> LoopMutationsVisitor<'a> {
    /// Initialize the visitor.
    fn new(iter: &'a Expr, target: &'a Expr, index: &'a Expr) -> Self {
        Self {
            iter,
            target,
            index,
            mutations: HashMap::new(),
            branches: vec![0],
            branch: 0,
        }
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

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoopMutationsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // Ex) `del items[0]`
            Stmt::Delete(StmtDelete { range, targets }) => {
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

            // Ex) `if True: items.append(1)`
            Stmt::If(StmtIf {
                test,
                body,
                elif_else_clauses,
                ..
            }) => {
                // Handle the `if` branch.
                self.branch += 1;
                self.branches.push(self.branch);
                self.visit_expr(test);
                self.visit_body(body);
                self.branches.pop();

                // Handle the `elif` and `else` branches.
                for clause in elif_else_clauses {
                    self.branch += 1;
                    self.branches.push(self.branch);
                    if let Some(test) = &clause.test {
                        self.visit_expr(test);
                    }
                    self.visit_body(&clause.body);
                    self.branches.pop();
                }
            }

            // On break, clear the mutations for the current branch.
            Stmt::Break(_) | Stmt::Return(_) => {
                if let Some(mutations) = self.mutations.get_mut(&self.branch) {
                    mutations.clear();
                }
                visitor::walk_stmt(self, stmt);
            }

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
