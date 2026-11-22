use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, CmpOp, ExprKind};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// error at runtime if either the element being tested for membership (the
/// left-hand side) or any element of the sequence (the right-hand side)
/// is unhashable (like lists or dictionaries). While Ruff will attempt to
/// infer the hashability of both sides and skip the fix when it can determine
/// that either side is unhashable, it may not always be able to do so.
///
/// ## References
/// - [What’s New In Python 3.2](https://docs.python.org/3/whatsnew/3.2.html#optimizations)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.1.1", category = Category::Pedantic)]
pub(crate) struct LiteralMembership;

impl AlwaysFixableViolation for LiteralMembership {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use a set literal when testing for membership".to_string()
    }

    fn fix_title(&self) -> String {
        "Convert to `set`".to_string()
    }
}

/// PLR6201
pub(crate) fn literal_membership(checker: &Checker, compare: ast::ExprCompare<'_>) {
    let Some((left, CmpOp::In | CmpOp::NotIn, right)) = compare.as_single() else {
        return;
    };

    let elts = match right.classify() {
        ExprKind::List(expr_list) => expr_list.elts(),
        ExprKind::Tuple(expr_tuple) => expr_tuple.elts(),
        _ => return,
    };

    // Skip empty collections (#15729).
    if elts.is_empty() {
        return;
    }

    // If `left`, or any of the elements in `right`, are known to _not_ be hashable, return.
    if std::iter::once(left)
        .chain(elts)
        .any(|expr| match expr.classify() {
            // Expressions that are known _not_ to be hashable.
            ExprKind::List(_)
            | ExprKind::Set(_)
            | ExprKind::Dict(_)
            | ExprKind::ListComp(_)
            | ExprKind::SetComp(_)
            | ExprKind::DictComp(_)
            | ExprKind::Generator(_)
            | ExprKind::Await(_)
            | ExprKind::Yield(_)
            | ExprKind::YieldFrom(_) => true,
            // Expressions that can be _inferred_ not to be hashable.
            ExprKind::Name(name) => {
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

    let mut diagnostic = checker.report_diagnostic(LiteralMembership, right.range());

    let literal = checker.locator().slice(right);
    let set = format!("{{{}}}", &literal[1..literal.len() - 1]);
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        set,
        right.range(),
    )));
}
