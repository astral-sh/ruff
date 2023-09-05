use ruff_python_ast::{self as ast, ExceptHandler, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_starred;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for single-element tuples in exception handlers (e.g.,
/// `except (ValueError,):`).
///
/// ## Why is this bad?
/// A tuple with a single element can be more concisely and idiomatically
/// expressed as a single value.
///
/// ## Example
/// ```python
/// try:
///     ...
/// except (ValueError,):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[violation]
pub struct RedundantTupleInExceptionHandler {
    name: String,
}

impl AlwaysAutofixableViolation for RedundantTupleInExceptionHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("A length-one tuple literal is redundant in exception handlers")
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
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_: Some(type_),
            ..
        }) = handler
        else {
            continue;
        };
        let Expr::Tuple(ast::ExprTuple { elts, .. }) = type_.as_ref() else {
            continue;
        };
        let [elt] = elts.as_slice() else {
            continue;
        };
        let elt = map_starred(elt);
        let mut diagnostic = Diagnostic::new(
            RedundantTupleInExceptionHandler {
                name: checker.generator().expr(elt),
            },
            type_.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // If there's no space between the `except` and the tuple, we need to insert a space,
            // as in:
            // ```python
            // except(ValueError,):
            // ```
            // Otherwise, the output will be invalid syntax, since we're removing a set of
            // parentheses.
            let requires_space = checker
                .locator()
                .slice(TextRange::up_to(type_.start()))
                .chars()
                .last()
                .is_some_and(|char| char.is_ascii_alphabetic());
            let content = checker.generator().expr(elt);
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                if requires_space {
                    format!(" {content}")
                } else {
                    content
                },
                type_.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
