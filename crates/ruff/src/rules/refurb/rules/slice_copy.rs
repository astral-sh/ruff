use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::is_list;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::refurb::helpers::make_name_method_call_suggestion;

/// ## What it does
/// Checks for unbounded slice expressions to copy a list.
///
/// ## Why is this bad?
/// The `list#copy` method is more readable and consistent with copying other
/// types.
///
/// ## Known problems
/// Prone to false negatives due to type inference limitations.
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
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html#mutable-sequence-types)
#[violation]
pub struct SliceCopy;

impl Violation for SliceCopy {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `copy` method over slicing")
    }
    fn autofix_title(&self) -> Option<String> {
        Some("Replace with `copy()`".to_string())
    }
}

/// FURB145
pub(crate) fn slice_copy(checker: &mut Checker, value: &Expr) {
    fn check(value: &Expr, checker: &mut Checker) {
        if let Expr::Tuple(ast::ExprTuple { elts, .. }) = value {
            elts.iter().for_each(|elt| check(elt, checker));
        } else {
            let Some(name) = match_list_full_slice(value, checker.semantic()) else {
                return;
            };
            let mut diagnostic = Diagnostic::new(SliceCopy, value.range());
            if checker.patch(diagnostic.kind.rule()) {
                let replacement =
                    make_name_method_call_suggestion(name, "copy", checker.generator());
                diagnostic.set_fix(Fix::suggested(Edit::replacement(
                    replacement,
                    value.start(),
                    value.end(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        };
    }
    check(value, checker);
}

// Matches `obj[:]` where `obj` is a list.
fn match_list_full_slice<'a>(expr: &'a Expr, semantic: &SemanticModel) -> Option<&'a str> {
    // Check that it is `obj[:]`.
    let subscript = expr.as_subscript_expr()?;
    if !matches!(
        subscript.slice.as_ref(),
        Expr::Slice(ast::ExprSlice {
            lower: None,
            upper: None,
            step: None,
            range: _,
        })
    ) {
        return None;
    }
    let ast::ExprName { id: name, .. } = subscript.value.as_name_expr()?;

    // Check that `obj` is a list.
    // TODO(tjkuson): Improve type inference.
    let scope = semantic.current_scope();
    let bindings: Vec<&Binding> = scope
        .get_all(name)
        .map(|binding_id| semantic.binding(binding_id))
        .collect();
    let [binding] = bindings.as_slice() else {
        return None;
    };
    if !(is_list(binding, semantic)) {
        return None;
    }

    Some(name)
}
