use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for operations that checks a list or tuple for an element.
///
/// ## Why is this bad?
/// Membership tests are more efficient when performed on a
/// lookup-optimized datatype like `set`.
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
/// ## References
/// - [Python 3.2 release notes](https://docs.python.org/3/whatsnew/3.2.html#optimizations)
#[violation]
pub struct SetMembership;

impl AlwaysFixableViolation for SetMembership {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a `set` when checking for element membership")
    }

    fn fix_title(&self) -> String {
        format!("Use a `set` when checking for element membership")
    }
}

/// PLR6201
pub(crate) fn set_membership(checker: &mut Checker, compare: &ast::ExprCompare) {
    let [op] = compare.ops.as_slice() else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = compare.comparators.as_slice() else {
        return;
    };

    let (Expr::List(ast::ExprList {
        elts: right_elements,
        ..
    })
    | Expr::Tuple(ast::ExprTuple {
        elts: right_elements,
        ..
    })) = right
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(SetMembership, right.range());

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        checker.generator().expr(&Expr::Set(ast::ExprSet {
            elts: right_elements.clone(),
            range: TextRange::default(),
        })),
        right.range(),
    )));

    checker.diagnostics.push(diagnostic);
}
