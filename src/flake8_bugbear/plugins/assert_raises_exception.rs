use rustpython_ast::{ExprKind, Stmt, Withitem};

use crate::ast::helpers::match_module_member;
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
                && matches!(&func.node, ExprKind::Attribute { attr, .. } if attr == "assertRaises")
                && match_module_member(
                    args.first().unwrap(),
                    "",
                    "Exception",
                    &checker.from_imports,
                    &checker.import_aliases,
                )
            {
                checker.add_check(Check::new(
                    CheckKind::NoAssertRaisesException,
                    Range::from_located(stmt),
                ));
            }
        }
    }
}
