//! Checks for `self.assertRaises(Exception)`.
//!
//! ## Why is this bad?
//!
//! `assertRaises(Exception)` should be considered evil. It can lead to your
//! test passing even if the code being tested is never executed due to a
//! typo. Either assert for a more specific exception (builtin or
//! custom), use `assertRaisesRegex`, or use the context manager form of
//! `assertRaises`.
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{ExprKind, Stmt, Withitem};

define_violation!(
    pub struct NoAssertRaisesException;
);
impl Violation for NoAssertRaisesException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`assertRaises(Exception)` should be considered evil")
    }
}

/// B017
pub fn assert_raises_exception(checker: &mut Checker, stmt: &Stmt, items: &[Withitem]) {
    let Some(item) = items.first() else {
        return;
    };
    let item_context = &item.context_expr;
    let ExprKind::Call { func, args, .. } = &item_context.node else {
        return;
    };
    if args.len() != 1 {
        return;
    }
    if item.optional_vars.is_some() {
        return;
    }
    if !matches!(&func.node, ExprKind::Attribute { attr, .. } if attr == "assertRaises") {
        return;
    }
    if !checker
        .resolve_call_path(args.first().unwrap())
        .map_or(false, |call_path| call_path.as_slice() == ["", "Exception"])
    {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        NoAssertRaisesException,
        Range::from_located(stmt),
    ));
}
