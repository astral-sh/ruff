use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;

/// B013
pub fn redundant_tuple_in_exception_handler(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
        if let Some(type_) = type_ {
            if let ExprKind::Tuple { elts, .. } = &type_.node {
                if let [elt] = &elts[..] {
                    let mut check = Check::new(
                        CheckKind::RedundantTupleInExceptionHandler(elt.to_string()),
                        Range::from_located(type_),
                    );
                    if checker.patch(check.kind.code()) {
                        let mut generator = SourceGenerator::new();
                        if let Ok(()) = generator.unparse_expr(elt, 0) {
                            if let Ok(content) = generator.generate() {
                                check.amend(Fix::replacement(
                                    content,
                                    type_.location,
                                    type_.end_location.unwrap(),
                                ));
                            }
                        }
                    }
                    checker.add_check(check)
                }
            }
        }
    }
}
