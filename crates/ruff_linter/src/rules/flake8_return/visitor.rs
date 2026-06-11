use ruff_python_ast::{self as ast, ElifElseClause, Expr, Identifier, Stmt};
use rustc_hash::FxHashSet;

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

#[derive(Default)]
pub(super) struct Stack<'data> {
    /// The `return` statements in the current function.
    pub(super) returns: Vec<&'data ast::StmtReturn>,
    /// The `elif` or `else` statements in the current function.
    pub(super) elifs_elses: Vec<(&'data [Stmt], &'data ElifElseClause)>,
    /// The non-local variables in the current function.
    pub(super) non_locals: FxHashSet<&'data str>,
    /// The annotated variables in the current function.
    ///
    /// For example, consider:
    /// ```python
    /// x: int
    ///
    /// if True:
    ///    x = foo()
    ///    return x
    /// ```
    ///
    /// In this case, the annotation on `x` is used to cast the return value
    /// of `foo()` to an `int`. Removing the `x = foo()` statement would
    /// change the return type of the function.
    pub(super) annotations: FxHashSet<&'data str>,
    /// Whether the current function is a generator.
    pub(super) is_generator: bool,
    /// The `assignment`-to-`return` statement pairs in the current function, each paired with the
    /// ranges of any enclosing `finally` suites that run after the `return`.
    /// TODO(charlie): Remove the extra [`Stmt`] here, which is necessary to support statement
    /// removal for the `return` statement.
    pub(super) assignment_return: Vec<(
        &'data ast::StmtAssign,
        &'data ast::StmtReturn,
        &'data Stmt,
        Vec<TextRange>,
    )>,
}

pub(super) struct ReturnVisitor<'semantic, 'data> {
    /// The semantic model of the current file.
    semantic: &'semantic SemanticModel<'data>,
    /// The current stack of nodes.
    pub(super) stack: Stack<'data>,
    /// The preceding sibling of the current node.
    sibling: Option<&'data Stmt>,
    /// The parent nodes of the current node.
    parents: Vec<&'data Stmt>,
    /// Ranges of the `finally` suites that would run after a `return` reached here.
    enclosing_finally: Vec<TextRange>,
}

impl<'semantic, 'data> ReturnVisitor<'semantic, 'data> {
    pub(super) fn new(semantic: &'semantic SemanticModel<'data>) -> Self {
        Self {
            semantic,
            stack: Stack::default(),
            sibling: None,
            parents: Vec::new(),
            enclosing_finally: Vec::new(),
        }
    }
}

impl<'a> Visitor<'a> for ReturnVisitor<'_, 'a> {
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
            Stmt::Global(ast::StmtGlobal {
                names,
                range: _,
                node_index: _,
            })
            | Stmt::Nonlocal(ast::StmtNonlocal {
                names,
                range: _,
                node_index: _,
            }) => {
                self.stack
                    .non_locals
                    .extend(names.iter().map(Identifier::as_str));
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. })
                // Ex) `x: int`
                if value.is_none() => {
                    if let Expr::Name(name) = target.as_ref() {
                        self.stack.annotations.insert(name.id.as_str());
                    }
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
                            self.stack.assignment_return.push((
                                stmt_assign,
                                stmt_return,
                                stmt,
                                self.enclosing_finally.clone(),
                            ));
                        }
                        // Example:
                        // ```python
                        // def foo():
                        //     with open("foo.txt", "r") as f:
                        //         x = f.read()
                        //     return x
                        // ```
                        Stmt::With(with) => {
                            if let Some(stmt_assign) =
                                with.body.last().and_then(Stmt::as_assign_stmt)
                            {
                                if !has_conditional_body(with, self.semantic) {
                                    self.stack.assignment_return.push((
                                        stmt_assign,
                                        stmt_return,
                                        stmt,
                                        self.enclosing_finally.clone(),
                                    ));
                                }
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
            Stmt::Try(stmt_try) => {
                self.sibling = Some(stmt);
                self.parents.push(stmt);

                // The `finally` runs after a `return` in the `body`, `handlers`, or `orelse`, so
                // track its range while visiting those. Not the `finalbody` itself: its own
                // statements can't re-read a `return`.
                let finally_range = stmt_try
                    .finalbody
                    .first()
                    .zip(stmt_try.finalbody.last())
                    .map(|(first, last)| TextRange::new(first.start(), last.end()));
                if let Some(finally_range) = finally_range {
                    self.enclosing_finally.push(finally_range);
                }
                self.visit_body(&stmt_try.body);
                for handler in &stmt_try.handlers {
                    visitor::walk_except_handler(self, handler);
                }
                self.visit_body(&stmt_try.orelse);
                if finally_range.is_some() {
                    self.enclosing_finally.pop();
                }
                self.visit_body(&stmt_try.finalbody);

                self.parents.pop();
                return;
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

/// Returns `true` if the [`With`] statement is known to have a conditional body. In other words:
/// if the [`With`] statement's body may or may not run.
///
/// For example, in the following, it's unsafe to inline the `return` into the `with`, since if
/// `data.decode()` fails, the behavior of the program will differ. (As-is, the function will return
/// the input `data`; if we inline the `return`, the function will return `None`.)
///
/// ```python
/// def func(data):
///     with suppress(JSONDecoderError):
///         data = data.decode()
///     return data
/// ```
pub(crate) fn has_conditional_body(with: &ast::StmtWith, semantic: &SemanticModel) -> bool {
    with.items.iter().any(|item| {
        let ast::WithItem {
            context_expr: Expr::Call(ast::ExprCall { func, .. }),
            ..
        } = item
        else {
            return false;
        };
        if let Some(qualified_name) = semantic.resolve_qualified_name(func) {
            if qualified_name.segments() == ["contextlib", "suppress"] {
                return true;
            }
        }
        false
    })
}
