use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, ExceptHandler, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;

/// ## What it does
/// Checks for single-element tuples in exception handlers (e.g.,
/// `except (ValueError,):`).
///
/// Note: Single-element tuples consisting of a starred expression
/// are allowed.
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

impl AlwaysFixableViolation for RedundantTupleInExceptionHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("A length-one tuple literal is redundant in exception handlers")
    }

    fn fix_title(&self) -> String {
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
        // It is not safe to replace a single-element
        // tuple consisting of a starred expression
        // by the unstarred expression because the unstarred
        // expression can be any iterable whereas `except` must
        // be followed by a literal or a tuple. For example:
        // ```python
        // except (*[ValueError,FileNotFoundError],)
        // ```
        if elt.is_starred_expr() {
            continue;
        }
        let mut diagnostic = Diagnostic::new(
            RedundantTupleInExceptionHandler {
                name: checker.generator().expr(elt),
            },
            type_.range(),
        );
        // If there's no space between the `except` and the tuple, we need to insert a space,
        // as in:
        // ```python
        // except(ValueError,):
        // ```
        // Otherwise, the output will be invalid syntax, since we're removing a set of
        // parentheses.
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(
                checker.generator().expr(elt),
                type_.range(),
                checker.locator(),
            ),
            type_.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}
