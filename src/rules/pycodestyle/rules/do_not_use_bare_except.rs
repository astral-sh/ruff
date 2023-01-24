use rustpython_ast::{Excepthandler, Stmt, StmtKind};
use rustpython_parser::ast::Expr;

use crate::ast::helpers::except_range;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violations;

/// E722
pub fn do_not_use_bare_except(
    type_: Option<&Expr>,
    body: &[Stmt],
    handler: &Excepthandler,
    locator: &Locator,
) -> Option<Diagnostic> {
    if type_.is_none()
        && !body
            .iter()
            .any(|stmt| matches!(stmt.node, StmtKind::Raise { exc: None, .. }))
    {
        Some(Diagnostic::new(
            violations::DoNotUseBareExcept,
            except_range(handler, locator),
        ))
    } else {
        None
    }
}
