use ruff_python_ast::{
    ElifElseClause, Expr, ExprKind, Identifier, Stmt, StmtAssign, StmtKind, StmtReturn, StmtWith,
    Suite,
};
use rustc_hash::FxHashSet;

use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

#[derive(Default)]
pub(super) struct Stack<'data> {
    /// The `return` statements in the current function.
    pub(super) returns: Vec<StmtReturn<'data>>,
    /// The `elif` or `else` statements in the current function.
    pub(super) elifs_elses: Vec<(Suite<'data>, ElifElseClause<'data>)>,
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
    pub(super) assignment_return: Vec<AssignmentReturn<'data>>,
}

#[derive(Debug)]
pub(super) struct AssignmentReturn<'a> {
    pub(super) assign: StmtAssign<'a>,
    pub(super) return_: StmtReturn<'a>,
    pub(super) enclosing_finally: Vec<TextRange>,
}

pub(super) struct ReturnVisitor<'semantic, 'data> {
    /// The semantic model of the current file.
    semantic: &'semantic SemanticModel<'data>,
    /// The current stack of nodes.
    pub(super) stack: Stack<'data>,
    /// The preceding sibling of the current node.
    sibling: Option<Stmt<'data>>,
    /// The parent nodes of the current node.
    parents: Vec<Stmt<'data>>,
}

impl<'semantic, 'data> ReturnVisitor<'semantic, 'data> {
    pub(super) fn new(semantic: &'semantic SemanticModel<'data>) -> Self {
        Self {
            semantic,
            stack: Stack::default(),
            sibling: None,
            parents: Vec::new(),
        }
    }

    /// Return the enclosing `finally` suites that run after this `return`.
    fn enclosing_finally(&self, stmt_return: StmtReturn<'_>) -> Vec<TextRange> {
        self.parents
            .iter()
            .filter_map(|parent| parent.as_try_stmt())
            .filter_map(|stmt_try| {
                let first = stmt_try.finalbody().first()?;
                let last = stmt_try.finalbody().last()?;
                Some(TextRange::new(first.start(), last.end()))
            })
            .filter(|finally_range| !finally_range.contains_range(stmt_return.range()))
            .collect()
    }
}

impl<'a> Visitor<'a> for ReturnVisitor<'_, 'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        match stmt.classify() {
            StmtKind::ClassDef(class) => {
                // Visit the decorators, etc.
                self.sibling = Some(stmt);
                self.parents.push(stmt);
                for decorator in class.decorator_list() {
                    visitor::walk_decorator(self, decorator);
                }
                self.parents.pop();

                // But don't recurse into the body.
                return;
            }
            StmtKind::FunctionDef(function) => {
                // Visit the decorators, etc.
                self.sibling = Some(stmt);
                self.parents.push(stmt);
                for decorator in function.decorator_list() {
                    visitor::walk_decorator(self, decorator);
                }
                if let Some(returns) = function.returns() {
                    visitor::walk_expr(self, returns);
                }
                visitor::walk_parameters(self, function.parameters());
                self.parents.pop();

                // But don't recurse into the body.
                return;
            }
            StmtKind::Global(declaration) => {
                self.stack
                    .non_locals
                    .extend(declaration.names().iter().map(Identifier::as_str));
            }
            StmtKind::Nonlocal(declaration) => {
                self.stack
                    .non_locals
                    .extend(declaration.names().iter().map(Identifier::as_str));
            }
            // Ex) `x: int`
            StmtKind::AnnAssign(assign) if assign.value().is_none() => {
                if let Some(name) = assign.target().as_name_expr() {
                    self.stack.annotations.insert(name.id().as_str());
                }
            }
            StmtKind::Return(stmt_return) => {
                // If the `return` statement is preceded by an `assignment` statement, then the
                // `assignment` statement may be redundant.
                if let Some(sibling) = self.sibling {
                    match sibling.classify() {
                        // Example:
                        // ```python
                        // def foo():
                        //     x = 1
                        //     return x
                        // ```
                        StmtKind::Assign(stmt_assign) => {
                            let enclosing_finally = self.enclosing_finally(stmt_return);
                            self.stack.assignment_return.push(AssignmentReturn {
                                assign: stmt_assign,
                                return_: stmt_return,
                                enclosing_finally,
                            });
                        }
                        // Example:
                        // ```python
                        // def foo():
                        //     with open("foo.txt", "r") as f:
                        //         x = f.read()
                        //     return x
                        // ```
                        StmtKind::With(with) => {
                            if let Some(stmt_assign) = with
                                .body()
                                .last()
                                .and_then(ruff_python_ast::Stmt::as_assign_stmt)
                            {
                                if !has_conditional_body(with, self.semantic) {
                                    let enclosing_finally = self.enclosing_finally(stmt_return);
                                    self.stack.assignment_return.push(AssignmentReturn {
                                        assign: stmt_assign,
                                        return_: stmt_return,
                                        enclosing_finally,
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }

                self.stack.returns.push(stmt_return);
            }
            StmtKind::If(stmt_if) => {
                if let Some(first) = stmt_if.elif_else_clauses().first() {
                    self.stack.elifs_elses.push((stmt_if.body(), first));
                }
            }
            _ => {}
        }

        self.sibling = Some(stmt);
        self.parents.push(stmt);
        visitor::walk_stmt(self, stmt);
        self.parents.pop();
    }

    fn visit_expr(&mut self, expr: Expr<'a>) {
        match expr.classify() {
            ExprKind::YieldFrom(_) | ExprKind::Yield(_) => {
                self.stack.is_generator = true;
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_body(&mut self, body: Suite<'a>) {
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
pub(crate) fn has_conditional_body(with: StmtWith<'_>, semantic: &SemanticModel) -> bool {
    with.items().iter().any(|item| {
        let context_expr = item.context_expr();
        let Some(call) = context_expr.as_call_expr() else {
            return false;
        };
        let func = call.func();
        if let Some(qualified_name) = semantic.resolve_qualified_name(func) {
            if qualified_name.segments() == ["contextlib", "suppress"] {
                return true;
            }
        }
        false
    })
}
