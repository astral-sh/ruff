use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn blind_except(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_: Some(type_), .. } = &handler.node else {
            continue;
        };
        let ExprKind::Name { id, .. } = &type_.node else {
            continue;
        };
        for exception in ["BaseException", "Exception"] {
            if id == exception {
                checker.add_check(Check::new(
                    CheckKind::BlindExcept,
                    Range::from_located(type_),
                ));
            }
        }
    }
}
