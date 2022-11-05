use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B013
pub fn redundant_tuple_in_exception_handler(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if let Some(type_) = type_ {
            if let ExprKind::Tuple { elts, .. } = &type_.node {
                if elts.len() == 1 {
                    checker.add_check(Check::new(
                        CheckKind::RedundantTupleInExceptionHandler(elts[0].to_string()),
                        Range::from_located(type_),
                    ));
                }
            }
        }
    }
}
