use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Node;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Comprehension, Expr, ExprContext, Stmt};
use ruff_text_size::{TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions declared `async` that do not yield or otherwise use features requiring the
/// function to be declared `async`.
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
/// ```
/// ```
///
/// Use instead:
/// TODO
/// ```
/// ```
#[violation]
pub struct SpuriousAsync {
    name: String,
}

impl Violation for SpuriousAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SpuriousAsync{ name } = self;
        format!("Function `{name}` is declared `async`, but doesn't yield or use async features.")
    }
}

#[derive(Default)]
struct YieldingExprVisitor<'a> {
    yieldingExprs: Vec<&'a TextRange>,
}

impl<'a> Visitor<'a> for YieldingExprVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Await(ast::ExprAwait{ range, value }) => {
                self.yieldingExprs.push(range);
                visitor::walk_expr(self, value)
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// RUF029
pub(crate) fn spurious_async(checker: &mut Checker, is_async: bool, name: &str, body: &[Stmt], range: TextRange) {
    if !is_async {
        return;
    }

    let yields = {
        let mut visitor = YieldingExprVisitor::default();
        visitor.visit_body(body);
        visitor.yieldingExprs
    };
    if yields.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(SpuriousAsync{name:name.to_string()}, range));
    }
}
