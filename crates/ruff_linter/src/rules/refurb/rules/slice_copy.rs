use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, ExprSubscript};

use ruff_python_semantic::analyze::typing::is_list;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{Edit, Fix, FixAvailability, Violation};

use crate::rules::refurb::helpers::generate_method_call;

/// ## What it does
/// Checks for unbounded slice expressions to copy a list.
///
/// ## Why is this bad?
/// The `list.copy` method is more readable and consistent with copying other
/// types.
///
/// ## Known problems
/// This rule is prone to false negatives due to type inference limitations,
/// as it will only detect lists that are instantiated as literals or annotated
/// with a type annotation.
///
/// ## Example
/// ```python
/// a = [1, 2, 3]
/// b = a[:]
/// ```
///
/// Use instead:
/// ```python
/// a = [1, 2, 3]
/// b = a.copy()
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the slice expression contains comments.
///
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html#mutable-sequence-types)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.290", category = Category::Style)]
pub(crate) struct SliceCopy;

impl Violation for SliceCopy {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Prefer `copy` method over slicing".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `copy()`".to_string())
    }
}

/// FURB145
pub(crate) fn slice_copy(checker: &Checker, subscript: ExprSubscript<'_>) {
    if subscript.ctx().is_store() || subscript.ctx().is_del() {
        return;
    }

    let Some(name) = match_list_full_slice(subscript, checker.semantic()) else {
        return;
    };
    let mut diagnostic = checker.report_diagnostic(SliceCopy, subscript.range());
    let replacement = generate_method_call(name.as_str(), "copy", checker.generator());

    let applicability = if checker.comment_ranges().intersects(subscript.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    diagnostic.set_fix(Fix::applicable_edit(
        Edit::replacement(replacement, subscript.start(), subscript.end()),
        applicability,
    ));
}

/// Matches `obj[:]` where `obj` is a list.
fn match_list_full_slice<'a>(
    subscript: ExprSubscript<'a>,
    semantic: &SemanticModel,
) -> Option<&'a Name> {
    // Check that it is `obj[:]`.
    if !matches!(
        subscript.slice().fields(),
        ast::ExprFields::Slice(ast::ExprSliceFields {
            lower: None,
            upper: None,
            step: None,
            range: _,
            node: _,
        })
    ) {
        return None;
    }

    let expr_name = subscript.value().as_name_expr()?;
    let id = expr_name.id();

    // Check that `obj` is a list.
    let scope = semantic.current_scope();
    let bindings: Vec<&Binding> = scope
        .get_all(id)
        .map(|binding_id| semantic.binding(binding_id))
        .collect();
    let [binding] = bindings.as_slice() else {
        return None;
    };
    if !is_list(binding, semantic) {
        return None;
    }

    Some(id)
}
