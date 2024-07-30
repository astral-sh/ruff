use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing::{is_dict, is_list};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::refurb::helpers::generate_method_call;

/// ## What it does
/// Checks for `del` statements that delete the entire slice of a list or
/// dictionary.
///
/// ## Why is this bad?
/// It is faster and more succinct to remove all items via the `clear()`
/// method.
///
/// ## Known problems
/// This rule is prone to false negatives due to type inference limitations,
/// as it will only detect lists and dictionaries that are instantiated as
/// literals or annotated with a type annotation.
///
/// ## Example
/// ```python
/// names = {"key": "value"}
/// nums = [1, 2, 3]
///
/// del names[:]
/// del nums[:]
/// ```
///
/// Use instead:
/// ```python
/// names = {"key": "value"}
/// nums = [1, 2, 3]
///
/// names.clear()
/// nums.clear()
/// ```
///
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html?highlight=list#mutable-sequence-types)
/// - [Python documentation: `dict.clear()`](https://docs.python.org/3/library/stdtypes.html?highlight=list#dict.clear)
#[violation]
pub struct DeleteFullSlice;

impl Violation for DeleteFullSlice {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `clear` over deleting a full slice")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `clear()`".to_string())
    }
}

/// FURB131
pub(crate) fn delete_full_slice(checker: &mut Checker, delete: &ast::StmtDelete) {
    for target in &delete.targets {
        let Some(name) = match_full_slice(target, checker.semantic()) else {
            continue;
        };

        let mut diagnostic = Diagnostic::new(DeleteFullSlice, delete.range);

        // Fix is only supported for single-target deletions.
        if delete.targets.len() == 1 {
            let replacement = generate_method_call(name.id.clone(), "clear", checker.generator());
            diagnostic.set_fix(Fix::unsafe_edit(Edit::replacement(
                replacement,
                delete.start(),
                delete.end(),
            )));
        }

        checker.diagnostics.push(diagnostic);
    }
}

/// Match `del expr[:]` where `expr` is a list or a dict.
fn match_full_slice<'a>(expr: &'a Expr, semantic: &SemanticModel) -> Option<&'a ast::ExprName> {
    // Check that it is `del expr[...]`.
    let subscript = expr.as_subscript_expr()?;

    // Check that it is` `del expr[:]`.
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

    // It should only apply to variables that are known to be lists or dicts.
    let name = subscript.value.as_name_expr()?;
    let binding = semantic.binding(semantic.only_binding(name)?);
    if !(is_dict(binding, semantic) || is_list(binding, semantic)) {
        return None;
    }

    // Name is needed for the fix suggestion.
    Some(name)
}
