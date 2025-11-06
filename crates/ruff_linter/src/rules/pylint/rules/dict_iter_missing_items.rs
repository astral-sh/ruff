use ruff_python_ast::{Expr, Stmt};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for dictionary unpacking in a for loop without calling `.items()`.
///
/// ## Why is this bad?
/// When iterating over a dictionary in a for loop, if a dictionary is unpacked
/// without calling `.items()`, it could lead to a runtime error if the keys are not
/// a tuple of two elements.
///
/// It is likely that you're looking for an iteration over (key, value) pairs which
/// can only be achieved when calling `.items()`.
///
/// ## Example
/// ```python
/// data = {"Paris": 2_165_423, "New York City": 8_804_190, "Tokyo": 13_988_129}
///
/// for city, population in data:
///     print(f"{city} has population {population}.")
/// ```
///
/// Use instead:
/// ```python
/// data = {"Paris": 2_165_423, "New York City": 8_804_190, "Tokyo": 13_988_129}
///
/// for city, population in data.items():
///     print(f"{city} has population {population}.")
/// ```
///
/// ## Known problems
/// If the dictionary key is a tuple, e.g.:
///
/// ```python
/// d = {(1, 2): 3, (3, 4): 5}
/// for x, y in d:
///     print(x, y)
/// ```
///
/// The tuple key is unpacked into `x` and `y` instead of the key and values. This means that
/// the suggested fix of using `d.items()` would result in different runtime behavior. Ruff
/// cannot consistently infer the type of a dictionary's keys.
///
/// ## Fix safety
/// Due to the known problem with tuple keys, this fix is unsafe.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.3.0")]
pub(crate) struct DictIterMissingItems;

impl AlwaysFixableViolation for DictIterMissingItems {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unpacking a dictionary in iteration without calling `.items()`".to_string()
    }

    fn fix_title(&self) -> String {
        "Add a call to `.items()`".to_string()
    }
}

/// PLE1141
pub(crate) fn dict_iter_missing_items(checker: &Checker, target: &Expr, iter: &Expr) {
    let Expr::Tuple(tuple) = target else {
        return;
    };

    if tuple.len() != 2 {
        return;
    }

    let Expr::Name(name) = iter else {
        return;
    };

    let Some(binding) = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id))
    else {
        return;
    };
    if !is_dict(binding, checker.semantic()) {
        return;
    }

    // If we can reliably determine that a dictionary has keys that are tuples of two we don't warn
    if is_dict_key_tuple_with_two_elements(binding, checker.semantic()) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(DictIterMissingItems, iter.range());
    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        format!("{}.items()", name.id),
        iter.range(),
    )));
}

/// Returns true if the binding is a dictionary where each key is a tuple with two elements.
fn is_dict_key_tuple_with_two_elements(binding: &Binding, semantic: &SemanticModel) -> bool {
    let Some(statement) = binding.statement(semantic) else {
        return false;
    };

    let (dict_expr, annotation) = match statement {
        Stmt::Assign(assign_stmt) => {
            let Expr::Dict(dict_expr) = &*assign_stmt.value else {
                return false;
            };
            (dict_expr, None)
        }
        Stmt::AnnAssign(ann_assign_stmt) => {
            let Some(value) = ann_assign_stmt.value.as_ref() else {
                return false;
            };
            let Expr::Dict(dict_expr) = value.as_ref() else {
                return false;
            };
            (dict_expr, Some(ann_assign_stmt.annotation.as_ref()))
        }
        _ => return false,
    };

    // Check if dict is empty
    let is_empty = dict_expr.iter_keys().next().is_none();

    if is_empty {
        // For empty dicts, check type annotation
        if let Some(annotation) = annotation {
            // Check if annotation is dict[tuple[...], ...] where tuple has 2 elements
            return is_annotation_dict_with_tuple_keys(annotation, semantic);
        }
        // Empty dict without annotation should allow the fix
        return false;
    }

    // For non-empty dicts, check if all keys are 2-tuples
    dict_expr
        .iter_keys()
        .all(|key| matches!(key, Some(Expr::Tuple(tuple)) if tuple.len() == 2))
}

/// Returns true if the annotation is `dict[tuple[T1, T2], ...]` where tuple has exactly 2 elements.
fn is_annotation_dict_with_tuple_keys(annotation: &Expr, semantic: &SemanticModel) -> bool {
    // Handle stringized annotations
    let annotation = if let Expr::StringLiteral(_) = annotation {
        // For stringized annotations, we'd need to parse them, but for now,
        // we'll be conservative and return false (allow the fix)
        return false;
    } else {
        annotation
    };

    // Check if it's a subscript: dict[...]
    let Expr::Subscript(subscript) = annotation else {
        return false;
    };

    // Check if the value is `dict`
    let value_name = match subscript.value.as_ref() {
        Expr::Name(name) => name.id.as_str(),
        _ => return false,
    };

    // Check if it's dict or typing.Dict
    if value_name != "dict" && !semantic.match_typing_expr(subscript.value.as_ref(), "Dict") {
        return false;
    }

    // Extract the slice (should be a tuple: (key_type, value_type))
    let Expr::Tuple(tuple) = subscript.slice.as_ref() else {
        return false;
    };

    // dict[K, V] format - check if K is tuple with 2 elements
    if let Some(key_type) = tuple.elts.first() {
        return is_tuple_type_with_two_elements(key_type, semantic);
    }

    false
}

/// Returns true if the expression represents a tuple type with exactly 2 elements.
fn is_tuple_type_with_two_elements(expr: &Expr, semantic: &SemanticModel) -> bool {
    // Handle tuple[...] subscript
    if let Expr::Subscript(subscript) = expr {
        let value_name = match subscript.value.as_ref() {
            Expr::Name(name) => name.id.as_str(),
            _ => return false,
        };

        // Check if it's tuple or typing.Tuple
        if value_name == "tuple" || semantic.match_typing_expr(subscript.value.as_ref(), "Tuple") {
            // Check the slice - tuple[T1, T2] or tuple[T1, T2, ...]
            if let Expr::Tuple(tuple_slice) = subscript.slice.as_ref() {
                // For PEP 484: tuple[T1, T2, ...], the last element might be ...
                // For PEP 585: tuple[T1, T2], just check length
                let effective_len = tuple_slice
                    .elts
                    .iter()
                    .take_while(|elt| !matches!(elt, Expr::EllipsisLiteral(_)))
                    .count();
                return effective_len == 2;
            }
            return false;
        }
    }

    false
}
