use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    Expr, ExprAttribute, ExprBooleanLiteral, ExprCall, ExprDict, ExprList, ExprName,
    ExprNoneLiteral, ExprNumberLiteral, ExprSet, ExprStringLiteral, Int, Number,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for `dict.get(key, falsy_value)` used as implicit casts to boolean values.
///
/// ## Why is this bad?
/// The default fallback `None` is already falsy.
///
/// ## Example
///
/// ```python
/// if dict.get(key, False):
///     ...
/// ```
///
/// Use instead:
///
/// ```python
/// if dict.get(key):
///     ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the `dict.get()` call contains comments between arguments.
#[derive(ViolationMetadata)]
pub(crate) struct FalsyDictGetFallback;

impl AlwaysFixableViolation for FalsyDictGetFallback {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid providing a falsy fallback to `dict.get()` when used in a boolean context. The default fallback `None` is already falsy.".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove falsy fallback from `dict.get()`".to_string()
    }
}

pub(crate) fn falsy_dict_get_fallback(checker: &mut Checker, expr: &Expr) {
    // Check if the expression is a call
    let Expr::Call(ExprCall { func, .. }) = expr else {
        return;
    };

    // Check if the function being called is an attribute (e.g. `dict.get`)
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = &**func else {
        return;
    };

    // Ensure the method called is `get`
    if attr != "get" {
        return;
    }

    // Check if the object is a dictionary using the semantic model
    if let Expr::Name(expr_name) = &**value {
        if !is_known_to_be_of_type_dict(checker.semantic(), expr_name) {
            return;
        }
    } else {
        return;
    }

    let Expr::Call(ExprCall { arguments, .. }) = expr else {
        return;
    };

    // Check if a fallback arg is provided
    if arguments.args.len() < 2 {
        return;
    }

    // Get the fallback argument
    let fallback_arg = &arguments.args[1];

    // Define what is considered a falsy value
    let is_falsy = is_falsy_fallback(fallback_arg);

    if is_falsy {
        let diagnostic = Diagnostic::new(FalsyDictGetFallback, fallback_arg.range());

        if let Some(arg) = arguments.args.get(1) {
            if let Some(prev_arg) = arguments.args.first() {
                // Start from the end of the first argument (which is the key)
                let start = prev_arg.range().end();
                // End at the end of the fallback argument
                let end = arg.range().end();
                // Create an edit that deletes from the comma to the end of the fallback
                let edit = Edit::deletion(start, end);

                // Determine applicability based on the presence of comments
                let comment_ranges = checker.comment_ranges();
                let applicability = if comment_ranges.intersects(TextRange::new(start, end)) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };

                // Create an automatic fix with the deletion edit and appropriate applicability
                let fix = ruff_diagnostics::Fix::applicable_edit(edit, applicability);

                // Attach the fix to the diagnostic and add it to the checker's diagnostics
                checker.diagnostics.push(diagnostic.with_fix(fix));
                return;
            }
        }

        // If unable to determine the arguments correctly, push the diagnostic without a fix
        checker.diagnostics.push(diagnostic);
    }
}

/// Determines if the given expression is a falsy fallback value.
///
/// A falsy fallback is one of the following:
/// 1. `False`
/// 2. `""`
/// 3. `[]` or `list()`
/// 4. `{}` or `dict()`
/// 5. `set()`
/// 6. `0`
/// 7. `0.0`
/// 8. `None`
fn is_falsy_fallback(expr: &Expr) -> bool {
    match expr {
        // Handle boolean literals: False is falsy, True is truthy
        Expr::BooleanLiteral(ExprBooleanLiteral { value, .. }) => !*value,

        // Handle string literals: Empty string is falsy
        Expr::StringLiteral(ExprStringLiteral { value, .. }) => value.is_empty(),
        // Handle list literals: Empty list is falsy
        Expr::List(ExprList { elts, .. }) => elts.is_empty(),

        // Handle dict literals: Empty dict is falsy
        Expr::Dict(ExprDict { items, .. }) => items.is_empty(),

        // Handle set literals: Empty set is falsy
        Expr::Set(ExprSet { elts, .. }) => elts.is_empty(),

        // Handle integer or float literals: Zero is falsy
        Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => {
            *value == Number::Int(Int::ZERO) || *value == Number::Float(0.0)
        }

        // Handle None literal: None is falsy
        Expr::NoneLiteral(ExprNoneLiteral { .. }) => true,

        // Handle function calls like list(), dict(), set() with no arguments
        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            if let Expr::Name(ExprName { id, .. }) = &**func {
                matches!(id.as_str(), "list" | "dict" | "set") && arguments.args.is_empty()
            } else {
                false
            }
        }

        // All other expressions are considered truthy
        _ => false,
    }
}

fn is_known_to_be_of_type_dict(semantic: &SemanticModel, expr: &ExprName) -> bool {
    let Some(binding) = semantic.only_binding(expr).map(|id| semantic.binding(id)) else {
        return false;
    };

    typing::is_dict(binding, semantic)
}
