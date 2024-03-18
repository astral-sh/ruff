use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::RaiseStatementVisitor;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::Stmt;

const MAX_RAISES: usize = 3;

#[violation]
pub struct TooManyRaises(usize);

impl Violation for TooManyRaises {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many `raise` statements: ({} > {})", self.0, MAX_RAISES)
    }
}

pub(crate) fn too_many_raises(body: &[Stmt]) -> Option<Diagnostic> {
    let raises = {
        let mut visitor = RaiseStatementVisitor::default();
        visitor.visit_body(body);
        visitor.raises
    };

    raises
        .iter()
        .skip(MAX_RAISES)
        .take(1)
        .next()
        .map(|(range, _, _)| Diagnostic::new(TooManyRaises(raises.len()), *range))
}
