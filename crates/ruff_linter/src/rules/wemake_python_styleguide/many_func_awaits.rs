use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;
use ruff_python_ast::visitor::Visitor;


#[violation]
pub struct TooManyAwaits {
    awaits: usize,
    max_awaits: usize,
}


impl Violation for TooManyAwaits {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyAwaits {
            awaits,
            max_awaits
        } = self;
        format!("Found too many await expressions: ({awaits} > {max_awaits})")
    }
}


#[derive(Default)]
pub struct AwaitExprVisitor<'a> {
    pub awaits: Vec<&'a ast::ExprAwait>,
}


impl<'a> Visitor<'a> for AwaitExprVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.is_await_expr() {
            self.awaits.push(expr.as_await_expr().unwrap())
        }
    }
}


fn num_awaits(body: &[Stmt]) -> usize {
    let mut visitor = AwaitExprVisitor::default();
    visitor.visit_body(body);
    visitor.awaits.len()
}


pub(crate) fn too_many_awaits(function_def: &ast::StmtFunctionDef) -> Option<Diagnostic> {
    let awaits = num_awaits(function_def.body.as_slice());

    if awaits > 1 {
        Some(Diagnostic::new(TooManyAwaits { awaits, max_awaits: 1 }, TextRange::default()))
    } else { None }

}
