use rustpython_ast::{Located, StmtKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

/// S101
pub fn assert_used(stmt: &Located<StmtKind>) -> Check {
    Check::new(CheckKind::AssertUsed, Range::from_located(stmt))
}
