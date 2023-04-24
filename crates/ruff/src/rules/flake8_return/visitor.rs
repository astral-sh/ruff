use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{Expr, ExprKind, Location, Stmt, StmtKind};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

#[derive(Default)]
pub struct Stack<'a> {
    pub returns: Vec<(&'a Stmt, Option<&'a Expr>)>,
    pub yields: Vec<&'a Expr>,
    pub elses: Vec<&'a Stmt>,
    pub elifs: Vec<&'a Stmt>,
    pub references: FxHashMap<&'a str, Vec<Location>>,
    pub non_locals: FxHashSet<&'a str>,
    pub assignments: FxHashMap<&'a str, Vec<Location>>,
    pub loops: Vec<(Location, Location)>,
    pub tries: Vec<(Location, Location)>,
}

#[derive(Default)]
pub struct ReturnVisitor<'a> {
    pub stack: Stack<'a>,
    parents: Vec<&'a Stmt>,
}

impl<'a> ReturnVisitor<'a> {
    fn visit_assign_target(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Tuple { elts, .. } => {
                for elt in elts {
                    self.visit_assign_target(elt);
                }
                return;
            }
            ExprKind::Name { id, .. } => {
                self.stack
                    .assignments
                    .entry(id)
                    .or_insert_with(Vec::new)
                    .push(expr.location);
                return;
            }
            ExprKind::Attribute { .. } => {
                // Attribute assignments are often side-effects (e.g., `self.property = value`),
                // so we conservatively treat them as references to every known
                // variable.
                for name in self.stack.assignments.keys() {
                    self.stack
                        .references
                        .entry(name)
                        .or_insert_with(Vec::new)
                        .push(expr.location);
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

impl<'a> Visitor<'a> for ReturnVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::Global { names } | StmtKind::Nonlocal { names } => {
                self.stack
                    .non_locals
                    .extend(names.iter().map(String::as_str));
            }
            StmtKind::FunctionDef {
                decorator_list,
                args,
                returns,
                ..
            }
            | StmtKind::AsyncFunctionDef {
                decorator_list,
                args,
                returns,
                ..
            } => {
                // Don't recurse into the body, but visit the decorators, etc.
                for expr in decorator_list {
                    visitor::walk_expr(self, expr);
                }
                if let Some(returns) = returns {
                    visitor::walk_expr(self, returns);
                }
                visitor::walk_arguments(self, args);
            }
            StmtKind::Return { value } => {
                self.stack
                    .returns
                    .push((stmt, value.as_ref().map(|expr| &**expr)));

                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
            StmtKind::If { orelse, .. } => {
                let is_elif_arm = self.parents.iter().any(|parent| {
                    if let StmtKind::If { orelse, .. } = &parent.node {
                        orelse.len() == 1 && &orelse[0] == stmt
                    } else {
                        false
                    }
                });

                if !is_elif_arm {
                    let has_elif = orelse.len() == 1
                        && matches!(orelse.first().unwrap().node, StmtKind::If { .. });
                    let has_else = !orelse.is_empty();

                    if has_elif {
                        // `stmt` is an `if` block followed by an `elif` clause.
                        self.stack.elifs.push(stmt);
                    } else if has_else {
                        // `stmt` is an `if` block followed by an `else` clause.
                        self.stack.elses.push(stmt);
                    }
                }

                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
            StmtKind::Assign { targets, value, .. } => {
                if let ExprKind::Name { id, .. } = &value.node {
                    self.stack
                        .references
                        .entry(id)
                        .or_insert_with(Vec::new)
                        .push(value.location);
                }

                visitor::walk_expr(self, value);

                if let Some(target) = targets.first() {
                    // Skip unpacking assignments, like `x, y = my_object`.
                    if matches!(target.node, ExprKind::Tuple { .. })
                        && !matches!(value.node, ExprKind::Tuple { .. })
                    {
                        return;
                    }

                    self.visit_assign_target(target);
                }
            }
            StmtKind::For { .. } | StmtKind::AsyncFor { .. } | StmtKind::While { .. } => {
                self.stack
                    .loops
                    .push((stmt.location, stmt.end_location.unwrap()));

                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
            StmtKind::Try { .. } | StmtKind::TryStar { .. } => {
                self.stack
                    .tries
                    .push((stmt.location, stmt.end_location.unwrap()));

                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
            _ => {
                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Call { .. } => {
                // Arbitrary function calls can have side effects, so we conservatively treat
                // every function call as a reference to every known variable.
                for name in self.stack.assignments.keys() {
                    self.stack
                        .references
                        .entry(name)
                        .or_insert_with(Vec::new)
                        .push(expr.location);
                }
            }
            ExprKind::Name { id, .. } => {
                self.stack
                    .references
                    .entry(id)
                    .or_insert_with(Vec::new)
                    .push(expr.location);
            }
            ExprKind::YieldFrom { .. } | ExprKind::Yield { .. } => {
                self.stack.yields.push(expr);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
