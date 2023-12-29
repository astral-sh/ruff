use ruff_python_ast::{self as ast, Stmt};
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_text_size::TextRange;


#[violation]
pub struct TooManyAsserts {
    asserts: usize,
    max_asserts: usize,
}


impl Violation for TooManyAsserts {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyAsserts {
            asserts,
            max_asserts,
        } = self;
        format!("Found too many `assert` statements: ({asserts} > {max_asserts})")
    }
}


#[derive(Default)]
pub struct AssertStatementVisitor<'a> {
    pub asserts: Vec<&'a ast::StmtAssert>,
}


impl<'a, 'b> StatementVisitor<'b> for AssertStatementVisitor<'a>
    where
        'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match stmt {
            Stmt::Assert(stmt) => self.asserts.push(stmt),
            _ => walk_stmt(self, stmt),
        }
    }
}


fn num_asserts(body: &[Stmt]) -> usize {
    let mut visitor = AssertStatementVisitor::default();
    visitor.visit_body(body);
    visitor.asserts.len()
}


pub(crate) fn too_many_asserts(function_def: &ast::StmtFunctionDef) -> Option<Diagnostic> {
    let asserts = num_asserts(function_def.body.as_slice());

    if asserts > 1 {
        Some(Diagnostic::new(TooManyAsserts { asserts, max_asserts: 1 }, TextRange::default()))
    } else { None }
}
