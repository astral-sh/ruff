use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::source_code_generator::SourceCodeGenerator;
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
        let mut check = Diagnostic::new(
            violations::RedundantTupleInExceptionHandler(elt.to_string()),
            Range::from_located(type_),
        );
        if checker.patch(check.kind.code()) {
            let mut generator: SourceCodeGenerator = checker.style.into();
            generator.unparse_expr(elt, 0);
            check.amend(Fix::replacement(
                generator.generate(),
                type_.location,
                type_.end_location.unwrap(),
            ));
        }
        checker.checks.push(check);
    }
}
