use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;

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
        let mut check = Check::new(
            CheckKind::RedundantTupleInExceptionHandler(elt.to_string()),
            Range::from_located(type_),
        );
        if checker.patch(check.kind.code()) {
            let mut generator = SourceCodeGenerator::new(
                checker.style.indentation(),
                checker.style.quote(),
                checker.style.line_ending(),
            );
            generator.unparse_expr(elt, 0);
            if let Ok(content) = generator.generate() {
                check.amend(Fix::replacement(
                    content,
                    type_.location,
                    type_.end_location.unwrap(),
                ));
            }
        }
        checker.add_check(check);
    }
}
