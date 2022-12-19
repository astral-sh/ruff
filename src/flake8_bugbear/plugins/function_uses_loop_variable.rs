use rustc_hash::FxHashSet;
use rustpython_ast::{Comprehension, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::collect_arg_names;
use crate::ast::types::{Node, Range};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

#[derive(Default)]
struct LoadedNamesVisitor<'a> {
    // Tuple of: name, defining expression, and defining range.
    names: Vec<(&'a str, &'a Expr, Range)>,
    // If we're in an f-string, the range of the defining expression.
    in_f_string: Option<Range>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a, 'b> Visitor<'b> for LoadedNamesVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::JoinedStr { .. } => {
                let prev_in_f_string = self.in_f_string;
                self.in_f_string = Some(Range::from_located(expr));
                visitor::walk_expr(self, expr);
                self.in_f_string = prev_in_f_string;
            }
            ExprKind::Name { id, ctx } if matches!(ctx, ExprContext::Load) => {
                self.names.push((
                    id,
                    expr,
                    self.in_f_string
                        .unwrap_or_else(|| Range::from_located(expr)),
                ));
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct SuspiciousVariablesVisitor<'a> {
    names: Vec<(&'a str, &'a Expr, Range)>,
}

/// `Visitor` to collect all suspicious variables (those referenced in
/// functions, but not bound as arguments).
impl<'a, 'b> Visitor<'b> for SuspiciousVariablesVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { args, body, .. }
            | StmtKind::AsyncFunctionDef { args, body, .. } => {
                // Collect all loaded variable names.
                let mut visitor = LoadedNamesVisitor::default();
                for stmt in body {
                    visitor.visit_stmt(stmt);
                }

                // Collect all argument names.
                let arg_names = collect_arg_names(args);

                // Treat any non-arguments as "suspicious".
                self.names.extend(
                    visitor
                        .names
                        .into_iter()
                        .filter(|(id, ..)| !arg_names.contains(id)),
                );
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Lambda { args, body } => {
                // Collect all loaded variable names.
                let mut visitor = LoadedNamesVisitor::default();
                visitor.visit_expr(body);

                // Collect all argument names.
                let arg_names = collect_arg_names(args);

                // Treat any non-arguments as "suspicious".
                self.names.extend(
                    visitor
                        .names
                        .into_iter()
                        .filter(|(id, ..)| !arg_names.contains(id)),
                );
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct NamesFromAssignmentsVisitor<'a> {
    names: FxHashSet<&'a str>,
}

/// `Visitor` to collect all names used in an assignment expression.
impl<'a, 'b> Visitor<'b> for NamesFromAssignmentsVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Name { id, .. } => {
                self.names.insert(id.as_str());
            }
            ExprKind::Starred { value, .. } => {
                self.visit_expr(value);
            }
            ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
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
impl<'a, 'b> Visitor<'b> for AssignedNamesVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        if matches!(
            &stmt.node,
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. }
        ) {
            // Don't recurse.
            return;
        }

        match &stmt.node {
            StmtKind::Assign { targets, .. } => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                for expr in targets {
                    visitor.visit_expr(expr);
                }
                self.names.extend(visitor.names);
            }
            StmtKind::AugAssign { target, .. }
            | StmtKind::AnnAssign { target, .. }
            | StmtKind::For { target, .. }
            | StmtKind::AsyncFor { target, .. } => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                visitor.visit_expr(target);
                self.names.extend(visitor.names);
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        if matches!(&expr.node, ExprKind::Lambda { .. }) {
            // Don't recurse.
            return;
        }

        visitor::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: &'b Comprehension) {
        let mut visitor = NamesFromAssignmentsVisitor::default();
        visitor.visit_expr(&comprehension.target);
        self.names.extend(visitor.names);

        visitor::walk_comprehension(self, comprehension);
    }
}

/// B023
pub fn function_uses_loop_variable<'a, 'b>(checker: &'a mut Checker<'b>, node: &Node<'b>)
where
    'b: 'a,
{
    // Identify any "suspicious" variables. These are defined as variables that are
    // referenced in a function or lambda body, but aren't bound as arguments.
    let suspicious_variables = {
        let mut visitor = SuspiciousVariablesVisitor::<'b>::default();
        match node {
            Node::Stmt(stmt) => visitor.visit_stmt(stmt),
            Node::Expr(expr) => visitor.visit_expr(expr),
        }
        visitor.names
    };

    if !suspicious_variables.is_empty() {
        // Identify any variables that are assigned in the loop (ignoring functions).
        let reassigned_in_loop = {
            let mut visitor = AssignedNamesVisitor::<'b>::default();
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
                    checker.add_check(Check::new(
                        CheckKind::FunctionUsesLoopVariable(name.to_string()),
                        range,
                    ));
                }
            }
        }
    }
}
