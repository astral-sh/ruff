use ruff_python_ast::{self as ast, ElifElseClause, Expr, Identifier, Stmt};
use rustc_hash::FxHashSet;

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

#[derive(Default)]
pub(super) struct Stack<'a> {
    /// The `return` statements in the current function.
    pub(super) returns: Vec<&'a ast::StmtReturn>,
    /// The `elif` or `else` statements in the current function.
    pub(super) elifs_elses: Vec<(&'a [Stmt], &'a ElifElseClause)>,
    /// The non-local variables in the current function.
    pub(super) non_locals: FxHashSet<&'a str>,
    /// Whether the current function is a generator.
    pub(super) is_generator: bool,
    /// The `assignment`-to-`return` statement pairs in the current function.
    /// TODO(charlie): Remove the extra [`Stmt`] here, which is necessary to support statement
    /// removal for the `return` statement.
    pub(super) assignment_return: Vec<(&'a ast::StmtAssign, &'a ast::StmtReturn, &'a Stmt)>,
}

#[derive(Default)]
pub(super) struct ReturnVisitor<'a> {
    /// The current stack of nodes.
    pub(super) stack: Stack<'a>,
    /// The preceding sibling of the current node.
    sibling: Option<&'a Stmt>,
    /// The parent nodes of the current node.
    parents: Vec<&'a Stmt>,
}

impl<'a> Visitor<'a> for ReturnVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(ast::StmtClassDef { decorator_list, .. }) => {
                // Visit the decorators, etc.
                self.sibling = Some(stmt);
                self.parents.push(stmt);
                for decorator in decorator_list {
                    visitor::walk_decorator(self, decorator);
                }
                self.parents.pop();

                // But don't recurse into the body.
                return;
            }
            Stmt::FunctionDef(ast::StmtFunctionDef {
                parameters,
                decorator_list,
                returns,
                ..
            }) => {
                // Visit the decorators, etc.
                self.sibling = Some(stmt);
                self.parents.push(stmt);
                for decorator in decorator_list {
                    visitor::walk_decorator(self, decorator);
                }
                if let Some(returns) = returns {
                    visitor::walk_expr(self, returns);
                }
                visitor::walk_parameters(self, parameters);
                self.parents.pop();

                // But don't recurse into the body.
                return;
            }
            Stmt::Global(ast::StmtGlobal { names, range: _ })
            | Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                self.stack
                    .non_locals
                    .extend(names.iter().map(Identifier::as_str));
            }
            Stmt::Return(stmt_return) => {
                // If the `return` statement is preceded by an `assignment` statement, then the
                // `assignment` statement may be redundant.
                if let Some(sibling) = self.sibling {
                    match sibling {
                        // Example:
                        // ```python
                        // def foo():
                        //     x = 1
                        //     return x
                        // ```
                        Stmt::Assign(stmt_assign) => {
                            self.stack
                                .assignment_return
                                .push((stmt_assign, stmt_return, stmt));
                        }
                        // Example:
                        // ```python
                        // def foo():
                        //     with open("foo.txt", "r") as f:
                        //         x = f.read()
                        //     return x
                        // ```
                        Stmt::With(ast::StmtWith { body, .. }) => {
                            if let Some(stmt_assign) = body.last().and_then(Stmt::as_assign_stmt) {
                                self.stack
                                    .assignment_return
                                    .push((stmt_assign, stmt_return, stmt));
                            }
                        }
                        _ => {}
                    }
                }

                self.stack.returns.push(stmt_return);
            }
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                if let Some(first) = elif_else_clauses.first() {
                    self.stack.elifs_elses.push((body, first));
                }
            }
            _ => {}
        }

        self.sibling = Some(stmt);
        self.parents.push(stmt);
        visitor::walk_stmt(self, stmt);
        self.parents.pop();
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::YieldFrom(_) | Expr::Yield(_) => {
                self.stack.is_generator = true;
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_body(&mut self, body: &'a [Stmt]) {
        let sibling = self.sibling;
        self.sibling = None;
        visitor::walk_body(self, body);
        self.sibling = sibling;
    }
}
