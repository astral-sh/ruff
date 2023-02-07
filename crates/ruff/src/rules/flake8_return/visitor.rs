use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{Expr, ExprKind, Location, Stmt, StmtKind};

use crate::ast::visitor;
use crate::ast::visitor::Visitor;

#[derive(Default)]
pub struct Stack<'a> {
    pub returns: Vec<(&'a Stmt, Option<&'a Expr>)>,
    pub yields: Vec<&'a Expr>,
    pub ifs: Vec<&'a Stmt>,
    pub elifs: Vec<&'a Stmt>,
    pub refs: FxHashMap<&'a str, Vec<Location>>,
    pub non_locals: FxHashSet<&'a str>,
    pub assigns: FxHashMap<&'a str, Vec<Location>>,
    pub loops: Vec<(Location, Location)>,
    pub tries: Vec<(Location, Location)>,
}

#[derive(Default)]
pub struct ReturnVisitor<'a> {
    pub stack: Stack<'a>,
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
                    .assigns
                    .entry(id)
                    .or_insert_with(Vec::new)
                    .push(expr.location);
                return;
            }
            ExprKind::Attribute { .. } => {
                // Attribute assignments are often side-effects (e.g., `self.property = value`),
                // so we conservatively treat them as references to every known
                // variable.
                for name in self.stack.assigns.keys() {
                    self.stack
                        .refs
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
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                // Don't recurse.
            }
            StmtKind::Return { value } => {
                self.stack
                    .returns
                    .push((stmt, value.as_ref().map(|expr| &**expr)));
                visitor::walk_stmt(self, stmt);
            }
            StmtKind::If { orelse, .. } => {
                if orelse.len() == 1 && matches!(orelse.first().unwrap().node, StmtKind::If { .. })
                {
                    self.stack.elifs.push(stmt);
                } else {
                    self.stack.ifs.push(stmt);
                }
                visitor::walk_stmt(self, stmt);
            }
            StmtKind::Assign { targets, value, .. } => {
                if let ExprKind::Name { id, .. } = &value.node {
                    self.stack
                        .refs
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
                visitor::walk_stmt(self, stmt);
            }
            StmtKind::Try { .. } => {
                self.stack
                    .tries
                    .push((stmt.location, stmt.end_location.unwrap()));
                visitor::walk_stmt(self, stmt);
            }
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match &expr.node {
            ExprKind::Call { .. } => {
                // Arbitrary function calls can have side effects, so we conservatively treat
                // every function call as a reference to every known variable.
                for name in self.stack.assigns.keys() {
                    self.stack
                        .refs
                        .entry(name)
                        .or_insert_with(Vec::new)
                        .push(expr.location);
                }
            }
            ExprKind::Name { id, .. } => {
                self.stack
                    .refs
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
