use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Comprehension, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::collect_arg_names;
use ruff_python_ast::types::Node;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

use crate::checkers::ast::Checker;

#[violation]
pub struct FunctionUsesLoopVariable {
    name: String,
}

impl Violation for FunctionUsesLoopVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionUsesLoopVariable { name } = self;
        format!("Function definition does not bind loop variable `{name}`")
    }
}

#[derive(Default)]
struct LoadedNamesVisitor<'a> {
    // Tuple of: name, defining expression, and defining range.
    loaded: Vec<(&'a str, &'a Expr, TextRange)>,
    // Tuple of: name, defining expression, and defining range.
    stored: Vec<(&'a str, &'a Expr, TextRange)>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoadedNamesVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Name(ast::ExprName { id, ctx }) => match ctx {
                ExprContext::Load => self.loaded.push((id, expr, expr.range())),
                ExprContext::Store => self.stored.push((id, expr, expr.range())),
                ExprContext::Del => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct SuspiciousVariablesVisitor<'a> {
    names: Vec<(&'a str, &'a Expr, TextRange)>,
    safe_functions: Vec<&'a Expr>,
}

/// `Visitor` to collect all suspicious variables (those referenced in
/// functions, but not bound as arguments).
impl<'a> Visitor<'a> for SuspiciousVariablesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef(ast::StmtFunctionDef { args, body, .. })
            | StmtKind::AsyncFunctionDef(ast::StmtAsyncFunctionDef { args, body, .. }) => {
                // Collect all loaded variable names.
                let mut visitor = LoadedNamesVisitor::default();
                visitor.visit_body(body);

                // Collect all argument names.
                let mut arg_names = collect_arg_names(args);
                arg_names.extend(visitor.stored.iter().map(|(id, ..)| id));

                // Treat any non-arguments as "suspicious".
                self.names.extend(
                    visitor
                        .loaded
                        .into_iter()
                        .filter(|(id, ..)| !arg_names.contains(id)),
                );
                return;
            }
            StmtKind::Return(ast::StmtReturn { value: Some(value) }) => {
                // Mark `return lambda: x` as safe.
                if matches!(value.node, ExprKind::Lambda(_)) {
                    self.safe_functions.push(value);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Call(ast::ExprCall {
                func,
                args,
                keywords,
            }) => {
                if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
                    let id = id.as_str();
                    if id == "filter" || id == "reduce" || id == "map" {
                        for arg in args {
                            if matches!(arg.node, ExprKind::Lambda(_)) {
                                self.safe_functions.push(arg);
                            }
                        }
                    }
                }
                if let ExprKind::Attribute(ast::ExprAttribute { value, attr, .. }) = &func.node {
                    if attr == "reduce" {
                        if let ExprKind::Name(ast::ExprName { id, .. }) = &value.node {
                            if id == "functools" {
                                for arg in args {
                                    if matches!(arg.node, ExprKind::Lambda(_)) {
                                        self.safe_functions.push(arg);
                                    }
                                }
                            }
                        }
                    }
                }
                for keyword in keywords {
                    if keyword.node.arg.as_ref().map_or(false, |arg| arg == "key")
                        && matches!(keyword.node.value.node, ExprKind::Lambda(_))
                    {
                        self.safe_functions.push(&keyword.node.value);
                    }
                }
            }
            ExprKind::Lambda(ast::ExprLambda { args, body }) => {
                if !self.safe_functions.contains(&expr) {
                    // Collect all loaded variable names.
                    let mut visitor = LoadedNamesVisitor::default();
                    visitor.visit_expr(body);

                    // Collect all argument names.
                    let mut arg_names = collect_arg_names(args);
                    arg_names.extend(visitor.stored.iter().map(|(id, ..)| id));

                    // Treat any non-arguments as "suspicious".
                    self.names.extend(
                        visitor
                            .loaded
                            .iter()
                            .filter(|(id, ..)| !arg_names.contains(id)),
                    );

                    return;
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

#[derive(Default)]
struct NamesFromAssignmentsVisitor<'a> {
    names: FxHashSet<&'a str>,
}

/// `Visitor` to collect all names used in an assignment expression.
impl<'a> Visitor<'a> for NamesFromAssignmentsVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Name(ast::ExprName { id, .. }) => {
                self.names.insert(id.as_str());
            }
            ExprKind::Starred(ast::ExprStarred { value, .. }) => {
                self.visit_expr(value);
            }
            ExprKind::List(ast::ExprList { elts, .. })
            | ExprKind::Tuple(ast::ExprTuple { elts, .. }) => {
                for expr in elts {
                    self.visit_expr(expr);
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct AssignedNamesVisitor<'a> {
    names: FxHashSet<&'a str>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for AssignedNamesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if matches!(
            &stmt.node,
            StmtKind::FunctionDef(_) | StmtKind::AsyncFunctionDef(_)
        ) {
            // Don't recurse.
            return;
        }

        match &stmt.node {
            StmtKind::Assign(ast::StmtAssign { targets, .. }) => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                for expr in targets {
                    visitor.visit_expr(expr);
                }
                self.names.extend(visitor.names);
            }
            StmtKind::AugAssign(ast::StmtAugAssign { target, .. })
            | StmtKind::AnnAssign(ast::StmtAnnAssign { target, .. })
            | StmtKind::For(ast::StmtFor { target, .. })
            | StmtKind::AsyncFor(ast::StmtAsyncFor { target, .. }) => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                visitor.visit_expr(target);
                self.names.extend(visitor.names);
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if matches!(&expr.node, ExprKind::Lambda(_)) {
            // Don't recurse.
            return;
        }

        visitor::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        let mut visitor = NamesFromAssignmentsVisitor::default();
        visitor.visit_expr(&comprehension.target);
        self.names.extend(visitor.names);

        visitor::walk_comprehension(self, comprehension);
    }
}

/// B023
pub(crate) fn function_uses_loop_variable<'a>(checker: &mut Checker<'a>, node: &Node<'a>) {
    // Identify any "suspicious" variables. These are defined as variables that are
    // referenced in a function or lambda body, but aren't bound as arguments.
    let suspicious_variables = {
        let mut visitor = SuspiciousVariablesVisitor::default();
        match node {
            Node::Stmt(stmt) => visitor.visit_stmt(stmt),
            Node::Expr(expr) => visitor.visit_expr(expr),
        }
        visitor.names
    };

    if !suspicious_variables.is_empty() {
        // Identify any variables that are assigned in the loop (ignoring functions).
        let reassigned_in_loop = {
            let mut visitor = AssignedNamesVisitor::default();
            match node {
                Node::Stmt(stmt) => visitor.visit_stmt(stmt),
                Node::Expr(expr) => visitor.visit_expr(expr),
            }
            visitor.names
        };

        // If a variable was used in a function or lambda body, and assigned in the
        // loop, flag it.
        for (name, expr, range) in suspicious_variables {
            if reassigned_in_loop.contains(name) {
                if !checker.flake8_bugbear_seen.contains(&expr) {
                    checker.flake8_bugbear_seen.push(expr);
                    checker.diagnostics.push(Diagnostic::new(
                        FunctionUsesLoopVariable {
                            name: name.to_string(),
                        },
                        range,
                    ));
                }
            }
        }
    }
}
