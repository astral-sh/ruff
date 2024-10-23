use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_python_semantic::analyze::typing;
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
/// or dictionaries). While Ruff will attempt to infer the hashability of the
/// elements, it may not always be able to do so.
///
/// ## References
/// - [What’s New In Python 3.2](https://docs.python.org/3/whatsnew/3.2.html#optimizations)
#[violation]
pub struct LiteralMembership;

impl AlwaysFixableViolation for LiteralMembership {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a set literal when testing for membership")
    }

    fn fix_title(&self) -> String {
        format!("Convert to `set`")
    }
}

/// PLR6201
pub(crate) fn literal_membership(checker: &mut Checker, compare: &ast::ExprCompare) {
    let [op] = &*compare.ops else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = &*compare.comparators else {
        return;
    };

    let elts = match right {
        Expr::List(ast::ExprList { elts, .. }) => elts,
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts,
        _ => return,
    };

    // If `left`, or any of the elements in `right`, are known to _not_ be hashable, return.
    if std::iter::once(compare.left.as_ref())
        .chain(elts)
        .any(|expr| match expr {
            // Expressions that are known _not_ to be hashable.
            Expr::List(_)
            | Expr::Set(_)
            | Expr::Dict(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_) => true,
            // Expressions that can be _inferred_ not to be hashable.
            Expr::Name(name) => {
                let Some(id) = checker.semantic().resolve_name(name) else {
                    return false;
                };
                let binding = checker.semantic().binding(id);
                typing::is_list(binding, checker.semantic())
                    || typing::is_dict(binding, checker.semantic())
                    || typing::is_set(binding, checker.semantic())
            }
            _ => false,
        })
    {
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
