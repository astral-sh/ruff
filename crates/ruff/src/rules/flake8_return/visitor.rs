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
    /// The names that are assigned to in the current scope (e.g., anything on the left-hand side of
    /// an assignment).
    pub(crate) assigned_names: FxHashSet<&'a str>,
    /// The names that are declared in the current scope, and the ranges of those declarations
    /// (e.g., assignments, but also function and class definitions).
    pub(crate) declarations: FxHashMap<&'a str, Vec<TextSize>>,
    pub(crate) references: FxHashMap<&'a str, Vec<TextSize>>,
    pub(crate) non_locals: FxHashSet<&'a str>,
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
                self.stack.assigned_names.insert(id.as_str());
                self.stack
                    .declarations
                    .entry(id)
                    .or_insert_with(Vec::new)
                    .push(expr.start());
                return;
            }
            Expr::Attribute(_) => {
                // Attribute assignments are often side-effects (e.g., `self.property = value`),
                // so we conservatively treat them as references to every known
                // variable.
                for name in self.stack.declarations.keys() {
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
            Stmt::ClassDef(ast::StmtClassDef {
                decorator_list,
                name,
                ..
            }) => {
                // Mark a declaration.
                self.stack
                    .declarations
                    .entry(name.as_str())
                    .or_insert_with(Vec::new)
                    .push(stmt.start());

                // Don't recurse into the body, but visit the decorators, etc.
                for decorator in decorator_list {
                    visitor::walk_decorator(self, decorator);
                }
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                name,
                args,
                decorator_list,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                name,
                args,
                decorator_list,
                returns,
                ..
            }) => {
                // Mark a declaration.
                self.stack
                    .declarations
                    .entry(name.as_str())
                    .or_insert_with(Vec::new)
                    .push(stmt.start());

                // Don't recurse into the body, but visit the decorators, etc.
                for decorator in decorator_list {
                    visitor::walk_decorator(self, decorator);
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
                    if target.is_tuple_expr() && !value.is_tuple_expr() {
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
                for name in self.stack.declarations.keys() {
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
