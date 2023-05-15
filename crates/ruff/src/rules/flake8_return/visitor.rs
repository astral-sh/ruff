use ruff_text_size::{TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{self, Expr, Identifier, Ranged, Stmt};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

#[derive(Default)]
pub(crate) struct Stack<'a> {
    pub(crate) returns: Vec<(&'a Stmt, Option<&'a Expr>)>,
    pub(crate) yields: Vec<&'a Expr>,
    pub(crate) elses: Vec<&'a Stmt>,
    pub(crate) elifs: Vec<&'a Stmt>,
    pub(crate) references: FxHashMap<&'a str, Vec<TextSize>>,
    pub(crate) non_locals: FxHashSet<&'a str>,
    pub(crate) assignments: FxHashMap<&'a str, Vec<TextSize>>,
    pub(crate) loops: Vec<TextRange>,
    pub(crate) tries: Vec<TextRange>,
}

#[derive(Default)]
pub(crate) struct ReturnVisitor<'a> {
    pub(crate) stack: Stack<'a>,
    parents: Vec<&'a Stmt>,
}

impl<'a> ReturnVisitor<'a> {
    fn visit_assign_target(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for elt in elts {
                    self.visit_assign_target(elt);
                }
                return;
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                self.stack
                    .assignments
                    .entry(id)
                    .or_insert_with(Vec::new)
                    .push(expr.start());
                return;
            }
            Expr::Attribute(_) => {
                // Attribute assignments are often side-effects (e.g., `self.property = value`),
                // so we conservatively treat them as references to every known
                // variable.
                for name in self.stack.assignments.keys() {
                    self.stack
                        .references
                        .entry(name)
                        .or_insert_with(Vec::new)
                        .push(expr.start());
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

impl<'a> Visitor<'a> for ReturnVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Global(ast::StmtGlobal { names, range: _ })
            | Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                self.stack
                    .non_locals
                    .extend(names.iter().map(Identifier::as_str));
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                decorator_list,
                args,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                decorator_list,
                args,
                returns,
                ..
            }) => {
                // Don't recurse into the body, but visit the decorators, etc.
                for expr in decorator_list {
                    visitor::walk_expr(self, expr);
                }
                if let Some(returns) = returns {
                    visitor::walk_expr(self, returns);
                }
                visitor::walk_arguments(self, args);
            }
            Stmt::Return(ast::StmtReturn { value, range: _ }) => {
                self.stack
                    .returns
                    .push((stmt, value.as_ref().map(|expr| &**expr)));

                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
            Stmt::If(ast::StmtIf { orelse, .. }) => {
                let is_elif_arm = self.parents.iter().any(|parent| {
                    if let Stmt::If(ast::StmtIf { orelse, .. }) = parent {
                        orelse.len() == 1 && &orelse[0] == stmt
                    } else {
                        false
                    }
                });

                if !is_elif_arm {
                    let has_elif =
                        orelse.len() == 1 && matches!(orelse.first().unwrap(), Stmt::If(_));
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
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                    self.stack
                        .references
                        .entry(id)
                        .or_insert_with(Vec::new)
                        .push(value.start());
                }

                visitor::walk_expr(self, value);

                if let Some(target) = targets.first() {
                    // Skip unpacking assignments, like `x, y = my_object`.
                    if matches!(target, Expr::Tuple(_)) && !value.is_tuple_expr() {
                        return;
                    }

                    self.visit_assign_target(target);
                }
            }
            Stmt::For(_) | Stmt::AsyncFor(_) | Stmt::While(_) => {
                self.stack.loops.push(stmt.range());

                self.parents.push(stmt);
                visitor::walk_stmt(self, stmt);
                self.parents.pop();
            }
            Stmt::Try(_) | Stmt::TryStar(_) => {
                self.stack.tries.push(stmt.range());

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
        match expr {
            Expr::Call(_) => {
                // Arbitrary function calls can have side effects, so we conservatively treat
                // every function call as a reference to every known variable.
                for name in self.stack.assignments.keys() {
                    self.stack
                        .references
                        .entry(name)
                        .or_insert_with(Vec::new)
                        .push(expr.start());
                }
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                self.stack
                    .references
                    .entry(id)
                    .or_insert_with(Vec::new)
                    .push(expr.start());
            }
            Expr::YieldFrom(_) | Expr::Yield(_) => {
                self.stack.yields.push(expr);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
