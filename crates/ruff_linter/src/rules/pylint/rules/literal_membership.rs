use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for membership tests on `list` and `tuple` literals.
///
/// ## Why is this bad?
/// When testing for membership in a static sequence, prefer a `set` literal
/// over a `list` or `tuple`, as Python optimizes `set` membership tests.
///
/// ## Example
/// ```python
/// 1 in [1, 2, 3]
/// ```
///
/// Use instead:
/// ```python
/// 1 in {1, 2, 3}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as the use of a `set` literal will
/// error at runtime if the sequence contains unhashable elements (like lists
/// or dictionaries).
///
/// ## References
/// - [What’s New In Python 3.2](https://docs.python.org/3/whatsnew/3.2.html#optimizations)
#[violation]
pub struct LiteralMembership;

impl AlwaysFixableViolation for LiteralMembership {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a `set` literal when testing for membership")
    }

    fn fix_title(&self) -> String {
        format!("Convert to `set`")
    }
}

/// PLR6201
pub(crate) fn literal_membership(checker: &mut Checker, compare: &ast::ExprCompare) {
    let [op] = compare.ops.as_slice() else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = compare.comparators.as_slice() else {
        return;
    };

    if !matches!(right, Expr::List(_) | Expr::Tuple(_)) {
        return;
    }

    let mut diagnostic = Diagnostic::new(LiteralMembership, right.range());

    let literal = checker.locator().slice(right);
    let set = format!("{{{}}}", &literal[1..literal.len() - 1]);
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        set,
        right.range(),
    )));

    checker.diagnostics.push(diagnostic);
}
