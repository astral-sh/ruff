use crate::ast::comparable::ComparableExpr;
use crate::ast::helpers::unparse_expr;
use crate::ast::types::{Node, Range};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::settings::hashable::HashableRegex;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprContext, ExprKind, Stmt, StmtKind, Withitem};
use serde::{Deserialize, Serialize};
use std::{fmt, iter};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub enum BindingKind {
    For,
    With,
    Assignment,
}

impl fmt::Display for BindingKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BindingKind::For => fmt.write_str("for loop"),
            BindingKind::With => fmt.write_str("with statement"),
            BindingKind::Assignment => fmt.write_str("assignment"),
        }
    }
}

define_violation!(
    /// ## What it does
    /// Checks for variables defined in `for` loops and `with` statements that
    /// get overwritten within the body, for example by another `for` loop or
    /// `with` statement or by direct assignment.
    ///
    /// ## Why is this bad?
    /// Redefinition of a loop variable inside the loop's body causes its value
    /// to differ from the original loop iteration for the remainder of the
    /// block, in a way that will likely cause bugs.
    ///
    /// In Python, unlike many other languages, `for` loops and `with`
    /// statements don't define their own scopes. Therefore, a nested loop that
    /// uses the same target variable name as an outer loop will reuse the same
    /// actual variable, and the value from the last iteration will "leak out"
    /// into the remainder of the enclosing loop.
    ///
    /// While this mistake is easy to spot in small examples, it can be hidden
    /// in larger blocks of code where the definition and redefinition of the
    /// variable may not be visible at the same time.
    ///
    /// ## Example
    /// ```python
    /// for i in range(10):
    ///     i = 9
    ///     print(i)  # prints 9 every iteration
    ///
    /// for i in range(10):
    ///     for i in range(10):  # original value overwritten
    ///         pass
    ///     print(i)  # also prints 9 every iteration
    ///
    /// with path1.open() as f:
    ///     with path2.open() as f:
    ///         f = path2.open()
    ///     print(f.readline())  # prints a line from path2
    /// ```
    pub struct RedefinedLoopName {
        pub name: String,
        pub outer_kind: BindingKind,
        pub inner_kind: BindingKind,
    }
);
impl Violation for RedefinedLoopName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedLoopName {
            name,
            outer_kind,
            inner_kind,
        } = self;
        format!("Outer {outer_kind} variable `{name}` overwritten by inner {inner_kind} target")
    }
}

struct ExprWithBindingKind<'a> {
    expr: &'a Expr,
    binding_kind: BindingKind,
}

struct InnerForWithAssignTargetsVisitor<'a> {
    dummy_variable_rgx: &'a HashableRegex,
    assignment_targets: Vec<ExprWithBindingKind<'a>>,
}

impl<'a, 'b> Visitor<'b> for InnerForWithAssignTargetsVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        // Collect target expressions.
        match &stmt.node {
            // For and async for.
            StmtKind::For { target, .. } | StmtKind::AsyncFor { target, .. } => {
                self.assignment_targets.extend(
                    assignment_targets_from_expr(target, self.dummy_variable_rgx).map(|expr| {
                        ExprWithBindingKind {
                            expr,
                            binding_kind: BindingKind::For,
                        }
                    }),
                );
            }
            // With.
            StmtKind::With { items, .. } => {
                self.assignment_targets.extend(
                    assignment_targets_from_with_items(items, self.dummy_variable_rgx).map(
                        |expr| ExprWithBindingKind {
                            expr,
                            binding_kind: BindingKind::With,
                        },
                    ),
                );
            }
            // Assignment, augmented assignment, and annotated assignment.
            StmtKind::Assign { targets, .. } => {
                self.assignment_targets.extend(
                    assignment_targets_from_assign_targets(targets, self.dummy_variable_rgx).map(
                        |expr| ExprWithBindingKind {
                            expr,
                            binding_kind: BindingKind::Assignment,
                        },
                    ),
                );
            }
            StmtKind::AugAssign { target, .. } | StmtKind::AnnAssign { target, .. } => {
                self.assignment_targets.extend(
                    assignment_targets_from_expr(target, self.dummy_variable_rgx).map(|expr| {
                        ExprWithBindingKind {
                            expr,
                            binding_kind: BindingKind::Assignment,
                        }
                    }),
                );
            }
            _ => {}
        }
        // Decide whether to recurse.
        match &stmt.node {
            // Don't recurse into blocks that create a new scope.
            StmtKind::ClassDef { .. } => {}
            StmtKind::FunctionDef { .. } => {}
            // Otherwise, do recurse.
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }
}

fn assignment_targets_from_expr<'a, U>(
    expr: &'a Expr<U>,
    dummy_variable_rgx: &'a HashableRegex,
) -> Box<dyn Iterator<Item = &'a Expr<U>> + 'a> {
    // The Box is necessary to ensure the match arms have the same return type - we can't use
    // a cast to "impl Iterator", since at the time of writing that is only allowed for
    // return types and argument types.
    match &expr.node {
        ExprKind::Attribute {
            ctx: ExprContext::Store,
            ..
        } => Box::new(iter::once(expr)),
        ExprKind::Subscript {
            ctx: ExprContext::Store,
            ..
        } => Box::new(iter::once(expr)),
        ExprKind::Starred {
            ctx: ExprContext::Store,
            value,
            ..
        } => Box::new(iter::once(&**value)),
        ExprKind::Name {
            ctx: ExprContext::Store,
            id,
            ..
        } => {
            // Ignore dummy variables.
            if dummy_variable_rgx.is_match(id) {
                Box::new(iter::empty())
            } else {
                Box::new(iter::once(expr))
            }
        }
        ExprKind::List {
            ctx: ExprContext::Store,
            elts,
            ..
        } => Box::new(
            elts.iter()
                .flat_map(|elt| assignment_targets_from_expr(elt, dummy_variable_rgx)),
        ),
        ExprKind::Tuple {
            ctx: ExprContext::Store,
            elts,
            ..
        } => Box::new(
            elts.iter()
                .flat_map(|elt| assignment_targets_from_expr(elt, dummy_variable_rgx)),
        ),
        _ => Box::new(iter::empty()),
    }
}

fn assignment_targets_from_with_items<'a, U>(
    items: &'a [Withitem<U>],
    dummy_variable_rgx: &'a HashableRegex,
) -> impl Iterator<Item = &'a Expr<U>> + 'a {
    items
        .iter()
        .filter_map(|item| {
            item.optional_vars
                .as_ref()
                .map(|expr| assignment_targets_from_expr(&**expr, dummy_variable_rgx))
        })
        .flatten()
}

fn assignment_targets_from_assign_targets<'a, U>(
    targets: &'a [Expr<U>],
    dummy_variable_rgx: &'a HashableRegex,
) -> impl Iterator<Item = &'a Expr<U>> + 'a {
    targets
        .iter()
        .flat_map(|target| assignment_targets_from_expr(target, dummy_variable_rgx))
}

/// PLW2901
pub fn redefined_loop_name<'a, 'b>(checker: &'a mut Checker<'b>, node: &Node<'b>) {
    let (outer_assignment_targets, inner_assignment_targets) = match node {
        Node::Stmt(stmt) => match &stmt.node {
            // With.
            StmtKind::With { items, body, .. } => {
                let outer_assignment_targets: Vec<ExprWithBindingKind<'a>> =
                    assignment_targets_from_with_items(items, &checker.settings.dummy_variable_rgx)
                        .map(|expr| ExprWithBindingKind {
                            expr,
                            binding_kind: BindingKind::With,
                        })
                        .collect();
                let mut visitor = InnerForWithAssignTargetsVisitor {
                    dummy_variable_rgx: &checker.settings.dummy_variable_rgx,
                    assignment_targets: vec![],
                };
                for stmt in body {
                    visitor.visit_stmt(stmt);
                }
                (outer_assignment_targets, visitor.assignment_targets)
            }
            // For and async for.
            StmtKind::For { target, body, .. } | StmtKind::AsyncFor { target, body, .. } => {
                let outer_assignment_targets: Vec<ExprWithBindingKind<'a>> =
                    assignment_targets_from_expr(target, &checker.settings.dummy_variable_rgx)
                        .map(|expr| ExprWithBindingKind {
                            expr,
                            binding_kind: BindingKind::For,
                        })
                        .collect();
                let mut visitor = InnerForWithAssignTargetsVisitor {
                    dummy_variable_rgx: &checker.settings.dummy_variable_rgx,
                    assignment_targets: vec![],
                };
                for stmt in body {
                    visitor.visit_stmt(stmt);
                }
                (outer_assignment_targets, visitor.assignment_targets)
            }
            _ => panic!(
                "redefined_loop_name called on Statement that is not a With, For, or AsyncFor"
            ),
        },
        Node::Expr(_) => panic!("redefined_loop_name called on Node that is not a Statement"),
    };

    for outer_assignment_target in &outer_assignment_targets {
        for inner_assignment_target in &inner_assignment_targets {
            // Compare the targets structurally.
            if ComparableExpr::from(outer_assignment_target.expr)
                .eq(&(ComparableExpr::from(inner_assignment_target.expr)))
            {
                checker.diagnostics.push(Diagnostic::new(
                    RedefinedLoopName {
                        name: unparse_expr(outer_assignment_target.expr, checker.stylist),
                        outer_kind: outer_assignment_target.binding_kind,
                        inner_kind: inner_assignment_target.binding_kind,
                    },
                    Range::from_located(inner_assignment_target.expr),
                ));
            }
        }
    }
}
