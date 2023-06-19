use rustpython_parser::ast::{self, ExceptHandler, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct RedundantTupleInExceptionHandler {
    name: String,
}

impl AlwaysAutofixableViolation for RedundantTupleInExceptionHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantTupleInExceptionHandler { name } = self;
        format!(
            "A length-one tuple literal is redundant. Write `except {name}` instead of `except \
             ({name},)`."
        )
    }

    fn autofix_title(&self) -> String {
        let RedundantTupleInExceptionHandler { name } = self;
        format!("Replace with `except {name}`")
    }
}

/// B013
pub(crate) fn redundant_tuple_in_exception_handler(
    checker: &mut Checker,
    handlers: &[ExceptHandler],
) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_: Some(type_), .. }) = handler else {
            continue;
        };
        let Expr::Tuple(ast::ExprTuple { elts, .. }) = type_.as_ref() else {
            continue;
        };
        let [elt] = &elts[..] else {
            continue;
        };
        let mut diagnostic = Diagnostic::new(
            RedundantTupleInExceptionHandler {
                name: checker.generator().expr(elt),
            },
            type_.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                checker.generator().expr(elt),
                type_.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
