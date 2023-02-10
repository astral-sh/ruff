use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Comprehension, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use crate::ast::helpers::collect_arg_names;
use crate::ast::types::{Node, Range};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct FunctionUsesLoopVariable {
        pub name: String,
    }
);
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
    loaded: Vec<(&'a str, &'a Expr, Range)>,
    // Tuple of: name, defining expression, and defining range.
    stored: Vec<(&'a str, &'a Expr, Range)>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a, 'b> Visitor<'b> for LoadedNamesVisitor<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Name { id, ctx } => match ctx {
                ExprContext::Load => self.loaded.push((id, expr, Range::from_located(expr))),
                ExprContext::Store => self.stored.push((id, expr, Range::from_located(expr))),
                ExprContext::Del => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct SuspiciousVariablesVisitor<'a> {
    names: Vec<(&'a str, &'a Expr, Range)>,
    safe_functions: Vec<&'a Expr>,
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
                visitor.visit_body(body);

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
            }
            StmtKind::Return { value: Some(value) } => {
                // Mark `return lambda: x` as safe.
                if matches!(value.node, ExprKind::Lambda { .. }) {
                    self.safe_functions.push(value);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'b Expr) {
        match &expr.node {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Name { id, .. } = &func.node {
                    if id == "filter" || id == "reduce" || id == "map" {
                        for arg in args {
                            if matches!(arg.node, ExprKind::Lambda { .. }) {
                                self.safe_functions.push(arg);
                            }
                        }
                    }
                }
                if let ExprKind::Attribute { value, attr, .. } = &func.node {
                    if attr == "reduce" {
                        if let ExprKind::Name { id, .. } = &value.node {
                            if id == "functools" {
                                for arg in args {
                                    if matches!(arg.node, ExprKind::Lambda { .. }) {
                                        self.safe_functions.push(arg);
                                    }
                                }
                            }
                        }
                    }
                }
                for keyword in keywords {
                    if keyword.node.arg.as_ref().map_or(false, |arg| arg == "key")
                        && matches!(keyword.node.value.node, ExprKind::Lambda { .. })
                    {
                        self.safe_functions.push(&keyword.node.value);
                    }
                }
            }
            ExprKind::Lambda { args, body } => {
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
