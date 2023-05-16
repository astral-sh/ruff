use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Comprehension, Expr, ExprContext, Ranged, Stmt};

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
    loaded: Vec<(&'a str, &'a Expr)>,
    // Tuple of: name, defining expression, and defining range.
    stored: Vec<(&'a str, &'a Expr)>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoadedNamesVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, ctx, range: _ }) => match ctx {
                ExprContext::Load => self.loaded.push((id, expr)),
                ExprContext::Store => self.stored.push((id, expr)),
                ExprContext::Del => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct SuspiciousVariablesVisitor<'a> {
    names: Vec<(&'a str, &'a Expr)>,
    safe_functions: Vec<&'a Expr>,
}

/// `Visitor` to collect all suspicious variables (those referenced in
/// functions, but not bound as arguments).
impl<'a> Visitor<'a> for SuspiciousVariablesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef { args, body, .. })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef { args, body, .. }) => {
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
            Stmt::Return(ast::StmtReturn {
                value: Some(value),
                range: _,
            }) => {
                // Mark `return lambda: x` as safe.
                if value.is_lambda_expr() {
                    self.safe_functions.push(value);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(ast::ExprCall {
                func,
                args,
                keywords,
                range: _,
            }) => {
                match func.as_ref() {
                    Expr::Name(ast::ExprName { id, .. }) => {
                        let id = id.as_str();
                        if id == "filter" || id == "reduce" || id == "map" {
                            for arg in args {
                                if matches!(arg, Expr::Lambda(_)) {
                                    self.safe_functions.push(arg);
                                }
                            }
                        }
                    }
                    Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                        if attr == "reduce" {
                            if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                                if id == "functools" {
                                    for arg in args {
                                        if arg.is_lambda_expr() {
                                            self.safe_functions.push(arg);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                for keyword in keywords {
                    if keyword.arg.as_ref().map_or(false, |arg| arg == "key")
                        && matches!(keyword.value, Expr::Lambda(_))
                    {
                        self.safe_functions.push(&keyword.value);
                    }
                }
            }
            Expr::Lambda(ast::ExprLambda {
                args,
                body,
                range: _,
            }) => {
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
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                self.names.insert(id.as_str());
            }
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.visit_expr(value);
            }
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
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
        if matches!(stmt, Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_)) {
            // Don't recurse.
            return;
        }

        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                for expr in targets {
                    visitor.visit_expr(expr);
                }
                self.names.extend(visitor.names);
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. })
            | Stmt::AnnAssign(ast::StmtAnnAssign { target, .. })
            | Stmt::For(ast::StmtFor { target, .. })
            | Stmt::AsyncFor(ast::StmtAsyncFor { target, .. }) => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                visitor.visit_expr(target);
                self.names.extend(visitor.names);
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if matches!(expr, Expr::Lambda(_)) {
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
        for (name, expr) in suspicious_variables {
            if reassigned_in_loop.contains(name) {
                if !checker.flake8_bugbear_seen.contains(&expr) {
                    checker.flake8_bugbear_seen.push(expr);
                    checker.diagnostics.push(Diagnostic::new(
                        FunctionUsesLoopVariable {
                            name: name.to_string(),
                        },
                        expr.range(),
                    ));
                }
            }
        }
    }
}
