use rustpython_ast::{ExprKind, Stmt, Withitem};

use crate::ast::helpers::match_name_or_attr;
use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B017
pub fn assert_raises_exception(checker: &mut Checker, stmt: &Stmt, items: &[Withitem]) {
    if let Some(item) = items.first() {
        let item_context = &item.context_expr;
        if let ExprKind::Call { func, args, .. } = &item_context.node {
            if args.len() == 1
                && item.optional_vars.is_none()
                && match_name_or_attr(func, "assertRaises")
                && match_name_or_attr(args.first().unwrap(), "Exception")
            {
                checker.add_check(Check::new(
                    CheckKind::NoAssertRaisesException,
                    Range::from_located(stmt),
                ));
            }
        }
    }
}
