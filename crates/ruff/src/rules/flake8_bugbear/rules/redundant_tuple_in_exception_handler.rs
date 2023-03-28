use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct RedundantTupleInExceptionHandler {
    pub name: String,
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
            RedundantTupleInExceptionHandler {
                name: unparse_expr(elt, checker.stylist),
            },
            Range::from(type_),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Edit::replacement(
                unparse_expr(elt, checker.stylist),
                type_.location,
                type_.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
