use rustpython_ast::{Located, StmtKind};

use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};
use crate::violations;

/// S101
pub fn assert_used(stmt: &Located<StmtKind>) -> Check {
    Check::new(violations::AssertUsed, Range::from_located(stmt))
}
