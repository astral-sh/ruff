use std::{fmt, iter};

use regex::Regex;
use ruff_python_ast::{self as ast, Arguments, Expr, ExprContext, Stmt, WithItem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
/// in larger blocks of code, where the definition and redefinition of the
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
#[violation]
pub struct RedefinedLoopName {
    name: String,
    outer_kind: OuterBindingKind,
    inner_kind: InnerBindingKind,
}

impl Violation for RedefinedLoopName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedLoopName {
            name,
            outer_kind,
            inner_kind,
        } = self;
        // Prefix the nouns describing the outer and inner kinds with "outer" and "inner"
        // to better distinguish them, but to avoid confusion, only do so if the outer and inner
        // kinds are equal. For example, instead of:
        //
        //    "Outer `for` loop variable `i` overwritten by inner assignment target."
        //
        // We have:
        //
        //    "`for` loop variable `i` overwritten by assignment target."
        //
        // While at the same time, we have:
        //
        //    "Outer `for` loop variable `i` overwritten by inner `for` loop target."
        //    "Outer `with` statement variable `f` overwritten by inner `with` statement target."

        if outer_kind == inner_kind {
            format!("Outer {outer_kind} variable `{name}` overwritten by inner {inner_kind} target")
        } else {
            format!("{outer_kind} variable `{name}` overwritten by {inner_kind} target")
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum OuterBindingKind {
    For,
    With,
}

impl fmt::Display for OuterBindingKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OuterBindingKind::For => fmt.write_str("`for` loop"),
            OuterBindingKind::With => fmt.write_str("`with` statement"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum InnerBindingKind {
    For,
    With,
    Assignment,
}

impl fmt::Display for InnerBindingKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InnerBindingKind::For => fmt.write_str("`for` loop"),
            InnerBindingKind::With => fmt.write_str("`with` statement"),
            InnerBindingKind::Assignment => fmt.write_str("assignment"),
        }
    }
}

impl PartialEq<InnerBindingKind> for OuterBindingKind {
    fn eq(&self, other: &InnerBindingKind) -> bool {
        matches!(
            (self, other),
            (OuterBindingKind::For, InnerBindingKind::For)
                | (OuterBindingKind::With, InnerBindingKind::With)
        )
    }
}

struct ExprWithOuterBindingKind<'a> {
    expr: &'a Expr,
    binding_kind: OuterBindingKind,
}

struct ExprWithInnerBindingKind<'a> {
    expr: &'a Expr,
    binding_kind: InnerBindingKind,
}

struct InnerForWithAssignTargetsVisitor<'a, 'b> {
    context: &'a SemanticModel<'b>,
    dummy_variable_rgx: &'a Regex,
    assignment_targets: Vec<ExprWithInnerBindingKind<'a>>,
}

impl<'a, 'b> StatementVisitor<'b> for InnerForWithAssignTargetsVisitor<'a, 'b> {
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        // Collect target expressions.
        match stmt {
            Stmt::For(ast::StmtFor { target, .. }) => {
                self.assignment_targets.extend(
                    assignment_targets_from_expr(target, self.dummy_variable_rgx).map(|expr| {
                        ExprWithInnerBindingKind {
                            expr,
                            binding_kind: InnerBindingKind::For,
                        }
                    }),
                );
            }
            Stmt::With(ast::StmtWith { items, .. }) => {
                self.assignment_targets.extend(
                    assignment_targets_from_with_items(items, self.dummy_variable_rgx).map(
                        |expr| ExprWithInnerBindingKind {
                            expr,
                            binding_kind: InnerBindingKind::With,
                        },
                    ),
                );
            }
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                // Check for single-target assignments which are of the
                // form `x = cast(..., x)`.
                if targets
                    .first()
                    .is_some_and(|target| assignment_is_cast_expr(value, target, self.context))
                {
                    return;
                }
                self.assignment_targets.extend(
                    assignment_targets_from_assign_targets(targets, self.dummy_variable_rgx).map(
                        |expr| ExprWithInnerBindingKind {
                            expr,
                            binding_kind: InnerBindingKind::Assignment,
                        },
                    ),
                );
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                self.assignment_targets.extend(
                    assignment_targets_from_expr(target, self.dummy_variable_rgx).map(|expr| {
                        ExprWithInnerBindingKind {
                            expr,
                            binding_kind: InnerBindingKind::Assignment,
                        }
                    }),
                );
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                if value.is_none() {
                    return;
                }
                self.assignment_targets.extend(
                    assignment_targets_from_expr(target, self.dummy_variable_rgx).map(|expr| {
                        ExprWithInnerBindingKind {
                            expr,
                            binding_kind: InnerBindingKind::Assignment,
                        }
                    }),
                );
            }
            _ => {}
        }
        // Decide whether to recurse.
        match stmt {
            // Don't recurse into blocks that create a new scope.
            Stmt::ClassDef(_) | Stmt::FunctionDef(_) => {}
            // Otherwise, do recurse.
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }
}

/// Checks whether the given assignment value is a `typing.cast` expression
/// and that the target name is the same as the argument name.
///
/// Example:
/// ```python
/// from typing import cast
///
/// x = cast(int, x)
/// ```
fn assignment_is_cast_expr(value: &Expr, target: &Expr, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    }) = value
    else {
        return false;
    };
    let Expr::Name(ast::ExprName { id: target_id, .. }) = target else {
        return false;
    };
    if args.len() != 2 {
        return false;
    }
    let Expr::Name(ast::ExprName { id: arg_id, .. }) = &args[1] else {
        return false;
    };
    if arg_id != target_id {
        return false;
    }
    semantic.match_typing_expr(func, "cast")
}

fn assignment_targets_from_expr<'a>(
    expr: &'a Expr,
    dummy_variable_rgx: &'a Regex,
) -> Box<dyn Iterator<Item = &'a Expr> + 'a> {
    // The Box is necessary to ensure the match arms have the same return type - we can't use
    // a cast to "impl Iterator", since at the time of writing that is only allowed for
    // return types and argument types.
    match expr {
        Expr::Attribute(ast::ExprAttribute {
            ctx: ExprContext::Store,
            ..
        }) => Box::new(iter::once(expr)),
        Expr::Subscript(ast::ExprSubscript {
            ctx: ExprContext::Store,
            ..
        }) => Box::new(iter::once(expr)),
        Expr::Starred(ast::ExprStarred {
            ctx: ExprContext::Store,
            value,
            range: _,
        }) => Box::new(iter::once(value.as_ref())),
        Expr::Name(ast::ExprName {
            ctx: ExprContext::Store,
            id,
            range: _,
        }) => {
            // Ignore dummy variables.
            if dummy_variable_rgx.is_match(id) {
                Box::new(iter::empty())
            } else {
                Box::new(iter::once(expr))
            }
        }
        Expr::List(ast::ExprList {
            ctx: ExprContext::Store,
            elts,
            range: _,
        }) => Box::new(
            elts.iter()
                .flat_map(|elt| assignment_targets_from_expr(elt, dummy_variable_rgx)),
        ),
        Expr::Tuple(ast::ExprTuple {
            ctx: ExprContext::Store,
            elts,
            range: _,
            parenthesized: _,
        }) => Box::new(
            elts.iter()
                .flat_map(|elt| assignment_targets_from_expr(elt, dummy_variable_rgx)),
        ),
        _ => Box::new(iter::empty()),
    }
}

fn assignment_targets_from_with_items<'a>(
    items: &'a [WithItem],
    dummy_variable_rgx: &'a Regex,
) -> impl Iterator<Item = &'a Expr> + 'a {
    items
        .iter()
        .filter_map(|item| {
            item.optional_vars
                .as_ref()
                .map(|expr| assignment_targets_from_expr(expr, dummy_variable_rgx))
        })
        .flatten()
}

fn assignment_targets_from_assign_targets<'a>(
    targets: &'a [Expr],
    dummy_variable_rgx: &'a Regex,
) -> impl Iterator<Item = &'a Expr> + 'a {
    targets
        .iter()
        .flat_map(|target| assignment_targets_from_expr(target, dummy_variable_rgx))
}

/// PLW2901
pub(crate) fn redefined_loop_name(checker: &mut Checker, stmt: &Stmt) {
    let (outer_assignment_targets, inner_assignment_targets) = match stmt {
        Stmt::With(ast::StmtWith { items, body, .. }) => {
            let outer_assignment_targets: Vec<ExprWithOuterBindingKind> =
                assignment_targets_from_with_items(items, &checker.settings.dummy_variable_rgx)
                    .map(|expr| ExprWithOuterBindingKind {
                        expr,
                        binding_kind: OuterBindingKind::With,
                    })
                    .collect();
            let mut visitor = InnerForWithAssignTargetsVisitor {
                context: checker.semantic(),
                dummy_variable_rgx: &checker.settings.dummy_variable_rgx,
                assignment_targets: vec![],
            };
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            (outer_assignment_targets, visitor.assignment_targets)
        }
        Stmt::For(ast::StmtFor { target, body, .. }) => {
            let outer_assignment_targets: Vec<ExprWithOuterBindingKind> =
                assignment_targets_from_expr(target, &checker.settings.dummy_variable_rgx)
                    .map(|expr| ExprWithOuterBindingKind {
                        expr,
                        binding_kind: OuterBindingKind::For,
                    })
                    .collect();
            let mut visitor = InnerForWithAssignTargetsVisitor {
                context: checker.semantic(),
                dummy_variable_rgx: &checker.settings.dummy_variable_rgx,
                assignment_targets: vec![],
            };
            for stmt in body {
                visitor.visit_stmt(stmt);
            }
            (outer_assignment_targets, visitor.assignment_targets)
        }
        _ => panic!("redefined_loop_name called on Statement that is not a `With` or `For`"),
    };

    let mut diagnostics = Vec::new();

    for outer_assignment_target in &outer_assignment_targets {
        for inner_assignment_target in &inner_assignment_targets {
            // Compare the targets structurally.
            if ComparableExpr::from(outer_assignment_target.expr)
                .eq(&(ComparableExpr::from(inner_assignment_target.expr)))
            {
                diagnostics.push(Diagnostic::new(
                    RedefinedLoopName {
                        name: checker.generator().expr(outer_assignment_target.expr),
                        outer_kind: outer_assignment_target.binding_kind,
                        inner_kind: inner_assignment_target.binding_kind,
                    },
                    inner_assignment_target.expr.range(),
                ));
            }
        }
    }

    checker.diagnostics.extend(diagnostics);
}
