use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::{Ranged, TextRange};

use crate::{checkers::ast::Checker, registry::AsRule};

/// ## What it does
/// Checks for iterations over `set` literals.
///
/// ## Why is this bad?
/// Iterating over a `set` is less efficient than iterating over a sequence
/// type, like `list` or `tuple`.
///
/// ## Example
/// ```python
/// for number in {1, 2, 3}:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// for number in (1, 2, 3):
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[violation]
pub struct IterationOverSet;

impl AlwaysFixableViolation for IterationOverSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a sequence type instead of a `set` when iterating over values")
    }

    fn fix_title(&self) -> String {
        format!("Use a sequence type instead of a `set` when iterating over values")
    }
}

/// PLC0208
pub(crate) fn iteration_over_set(checker: &mut Checker, expr: &Expr) {
    let Expr::Set(ast::ExprSet { elts, .. }) = expr else {
        return;
    };

    if elts.iter().any(Expr::is_starred_expr) {
        return;
    }

    let mut diagnostic = Diagnostic::new(IterationOverSet, expr.range());

    if checker.patch(diagnostic.kind.rule()) {
        let first = elts.first().unwrap();
        let last = elts.last().unwrap();

        let inner_slice = checker
            .locator()
            .slice(TextRange::new(first.range().start(), last.range().end()));

        let content = if elts.len() == 1 {
            // handle the case of a single element in a tuple, needs a trailing comma
            format!("({inner_slice},)")
        } else {
            format!("({inner_slice})")
        };

        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            content,
            expr.range(),
        )));
    }

    checker.diagnostics.push(diagnostic);
}
