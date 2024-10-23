use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprSubscript};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, settings::types::PythonVersion};

/// ## What it does
/// Checks for consistent style regarding whether nonempty tuples in subscripts
/// are parenthesized.
///
/// The exact nature of this violation depends on the setting
/// [`lint.ruff.parenthesize-tuple-in-subscript`]. By default, the use of
/// parentheses is considered a violation.
///
/// This rule is not applied inside "typing contexts" (type annotations,
/// type aliases and subscripted class bases), as these have their own specific
/// conventions around them.
///
/// ## Why is this bad?
/// It is good to be consistent and, depending on the codebase, one or the other
/// convention may be preferred.
///
/// ## Example
///
/// ```python
/// directions = {(0, 1): "North", (1, 0): "East", (0, -1): "South", (-1, 0): "West"}
/// directions[(0, 1)]
/// ```
///
/// Use instead (with default setting):
///
/// ```python
/// directions = {(0, 1): "North", (1, 0): "East", (0, -1): "South", (-1, 0): "West"}
/// directions[0, 1]
/// ```

#[violation]
pub struct IncorrectlyParenthesizedTupleInSubscript {
    prefer_parentheses: bool,
}

impl AlwaysFixableViolation for IncorrectlyParenthesizedTupleInSubscript {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.prefer_parentheses {
            format!("Use parentheses for tuples in subscripts.")
        } else {
            format!("Avoid parentheses for tuples in subscripts.")
        }
    }

    fn fix_title(&self) -> String {
        if self.prefer_parentheses {
            "Parenthesize the tuple.".to_string()
        } else {
            "Remove the parentheses.".to_string()
        }
    }
}

/// RUF031
pub(crate) fn subscript_with_parenthesized_tuple(checker: &mut Checker, subscript: &ExprSubscript) {
    let prefer_parentheses = checker.settings.ruff.parenthesize_tuple_in_subscript;

    let Expr::Tuple(tuple_subscript) = &*subscript.slice else {
        return;
    };

    if tuple_subscript.parenthesized == prefer_parentheses || tuple_subscript.is_empty() {
        return;
    }

    // Adding parentheses in the presence of a slice leads to a syntax error.
    if prefer_parentheses && tuple_subscript.iter().any(Expr::is_slice_expr) {
        return;
    }

    // Removing parentheses in the presence of unpacking leads
    // to a syntax error in Python 3.10.
    // This is no longer a syntax error starting in Python 3.11
    // see https://peps.python.org/pep-0646/#change-1-star-expressions-in-indexes
    if checker.settings.target_version <= PythonVersion::Py310
        && !prefer_parentheses
        && tuple_subscript.iter().any(Expr::is_starred_expr)
    {
        return;
    }

    // subscripts in annotations, type definitions or class bases are typing subscripts.
    // These have their own special conventions; skip applying the rule in these cases.
    let semantic = checker.semantic();
    if semantic.in_annotation() || semantic.in_type_definition() || semantic.in_class_base() {
        return;
    }

    let locator = checker.locator();
    let source_range = subscript.slice.range();
    let new_source = if prefer_parentheses {
        format!("({})", locator.slice(source_range))
    } else {
        locator.slice(source_range)[1..source_range.len().to_usize() - 1].to_string()
    };
    let edit = Edit::range_replacement(new_source, source_range);

    checker.diagnostics.push(
        Diagnostic::new(
            IncorrectlyParenthesizedTupleInSubscript { prefer_parentheses },
            source_range,
        )
        .with_fix(Fix::safe_edit(edit)),
    );
}
