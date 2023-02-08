use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExprKind, Stmt, Withitem};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    /// ### What it does
    /// Checks for `self.assertRaises(Exception)`.
    ///
    /// ### Why is this bad?
    /// `assertRaises(Exception)` can lead to your test passing even if the
    /// code being tested is never executed due to a typo.
    ///
    /// Either assert for a more specific exception (builtin or custom), use
    /// `assertRaisesRegex` or the context manager form of `assertRaises`.
    ///
    /// ### Example
    /// ```python
    /// self.assertRaises(Exception, foo)
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// self.assertRaises(SomeSpecificException, foo)
    /// ```
    pub struct AssertRaisesException;
);
impl Violation for AssertRaisesException {
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
        AssertRaisesException,
        Range::from_located(stmt),
    ));
}
