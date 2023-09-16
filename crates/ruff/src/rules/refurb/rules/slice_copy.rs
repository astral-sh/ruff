use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::is_list;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
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
pub(crate) fn slice_copy(checker: &mut Checker, subscript: &ast::ExprSubscript) {
    if subscript.ctx.is_store() || subscript.ctx.is_del() {
        return;
    }

    let Some(name) = match_list_full_slice(subscript, checker.semantic()) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(SliceCopy, subscript.range());
    if checker.patch(diagnostic.kind.rule()) {
        let replacement = generate_method_call(name, "copy", checker.generator());
        diagnostic.set_fix(Fix::suggested(Edit::replacement(
            replacement,
            subscript.start(),
            subscript.end(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// Matches `obj[:]` where `obj` is a list.
fn match_list_full_slice<'a>(
    subscript: &'a ast::ExprSubscript,
    semantic: &SemanticModel,
) -> Option<&'a str> {
    // Check that it is `obj[:]`.
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

    let ast::ExprName { id, .. } = subscript.value.as_name_expr()?;

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
