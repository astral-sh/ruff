use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, Stmt};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions declared `async` that do not yield or otherwise use features requiring the
/// function to be declared `async`.
///
/// ## Why is this bad?
/// Declaring a function `async` when it's not is usually a mistake, and will artificially limit the
/// contexts where that function may be called. In some cases, labeling a function `async` is
/// semantically meaningful (e.g. with the trio library).
///
/// ## Examples
/// ```python
/// async def foo():
///     bar()
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     bar()
/// ```
#[violation]
pub struct SpuriousAsync {
    name: String,
}

impl Violation for SpuriousAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SpuriousAsync { name } = self;
        format!("Function `{name}` is declared `async`, but doesn't yield or use async features.")
    }
}

#[derive(Default)]
struct YieldingExprVisitor {
    found_await_or_yield: bool,
}

impl<'a> Visitor<'a> for YieldingExprVisitor {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Await(_) => {
                self.found_await_or_yield = true;
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                self.found_await_or_yield = true;
            }
            Stmt::For(ast::StmtFor { is_async: true, .. }) => {
                self.found_await_or_yield = true;
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// RUF029
pub(crate) fn spurious_async(
    checker: &mut Checker,
    ast::StmtFunctionDef {
        is_async,
        name,
        body,
        range,
        ..
    }: &ast::StmtFunctionDef,
) {
    if !is_async {
        return;
    }

    let yields = {
        let mut visitor = YieldingExprVisitor::default();
        visitor.visit_body(&body);
        visitor.found_await_or_yield
    };

    if !yields {
        checker.diagnostics.push(Diagnostic::new(
            SpuriousAsync {
                name: name.to_string(),
            },
            *range,
        ));
    }
}
