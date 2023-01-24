use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

/// B013
pub fn redundant_tuple_in_exception_handler(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_: Some(type_), .. } = &handler.node else {
            continue;
        };
        let ExprKind::Tuple { elts, .. } = &type_.node else {
            continue;
        };
        let [elt] = &elts[..] else {
            continue;
        };
        let mut diagnostic = Diagnostic::new(
            violations::RedundantTupleInExceptionHandler(unparse_expr(elt, checker.stylist)),
            Range::from_located(type_),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                unparse_expr(elt, checker.stylist),
                type_.location,
                type_.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
