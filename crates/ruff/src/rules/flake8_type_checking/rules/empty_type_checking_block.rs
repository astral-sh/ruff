use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct EmptyTypeCheckingBlock;
);
impl Violation for EmptyTypeCheckingBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found empty type-checking block")
    }
}

/// TCH005
pub fn empty_type_checking_block(checker: &mut Checker, test: &Expr, body: &[Stmt]) {
    if checker.resolve_call_path(test).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TYPE_CHECKING"]
    }) {
        if body.len() == 1 && matches!(body[0].node, StmtKind::Pass) {
            checker.diagnostics.push(Diagnostic::new(
                EmptyTypeCheckingBlock,
                Range::from_located(&body[0]),
            ));
        }
    }
}
