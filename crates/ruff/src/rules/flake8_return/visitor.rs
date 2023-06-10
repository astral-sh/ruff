use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Identifier, Stmt};

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;

#[derive(Default)]
pub(crate) struct Stack<'a> {
    /// The `return` statements in the current function.
    pub(crate) returns: Vec<&'a ast::StmtReturn>,
    /// The `else` statements in the current function.
    pub(crate) elses: Vec<&'a ast::StmtIf>,
    /// The `elif` statements in the current function.
    pub(crate) elifs: Vec<&'a ast::StmtIf>,
    /// The non-local variables in the current function.
    pub(crate) non_locals: FxHashSet<&'a str>,
    /// Whether the current function is a generator.
    pub(crate) is_generator: bool,
    /// The `assignment`-to-`return` statement pairs in the current function.
    pub(crate) assignment_return: Vec<(&'a ast::StmtAssign, &'a ast::StmtReturn)>,
}

#[derive(Default)]
pub(crate) struct ReturnVisitor<'a> {
    /// The current stack of nodes.
    pub(crate) stack: Stack<'a>,
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
                args,
                decorator_list,
                returns,
                ..
            })
            | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
                args,
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
                visitor::walk_arguments(self, args);
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
                                .push((stmt_assign, stmt_return));
                        }
                        // Example:
                        // ```python
                        // def foo():
                        //     with open("foo.txt", "r") as f:
                        //         x = f.read()
                        //     return x
                        // ```
                        Stmt::With(ast::StmtWith { body, .. })
                        | Stmt::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
                            if let Some(stmt_assign) = body.last().and_then(Stmt::as_assign_stmt) {
                                self.stack
                                    .assignment_return
                                    .push((stmt_assign, stmt_return));
                            }
                        }
                        _ => {}
                    }
                }

                self.stack.returns.push(stmt_return);
            }
            Stmt::If(stmt_if) => {
                let is_elif_arm = self.parents.iter().any(|parent| {
                    if let Stmt::If(ast::StmtIf { orelse, .. }) = parent {
                        orelse.len() == 1 && &orelse[0] == stmt
                    } else {
                        false
                    }
                });

                if !is_elif_arm {
                    let has_elif =
                        stmt_if.orelse.len() == 1 && stmt_if.orelse.first().unwrap().is_if_stmt();
                    let has_else = !stmt_if.orelse.is_empty();

                    if has_elif {
                        // `stmt` is an `if` block followed by an `elif` clause.
                        self.stack.elifs.push(stmt_if);
                    } else if has_else {
                        // `stmt` is an `if` block followed by an `else` clause.
                        self.stack.elses.push(stmt_if);
                    }
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
