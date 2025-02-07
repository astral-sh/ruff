use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Arguments, Expr, Int};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest};

/// ## What it does
/// Checks for use of `zip()` to iterate over successive pairs of elements.
///
/// ## Why is this bad?
/// When iterating over successive pairs of elements, prefer
/// `itertools.pairwise()` over `zip()`.
///
/// `itertools.pairwise()` is more readable and conveys the intent of the code
/// more clearly.
///
/// ## Example
/// ```python
/// letters = "ABCD"
/// zip(letters, letters[1:])  # ("A", "B"), ("B", "C"), ("C", "D")
/// ```
///
/// Use instead:
/// ```python
/// from itertools import pairwise
///
/// letters = "ABCD"
/// pairwise(letters)  # ("A", "B"), ("B", "C"), ("C", "D")
/// ```
///
/// ## References
/// - [Python documentation: `itertools.pairwise`](https://docs.python.org/3/library/itertools.html#itertools.pairwise)
#[derive(ViolationMetadata)]
pub(crate) struct ZipInsteadOfPairwise;

impl Violation for ZipInsteadOfPairwise {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Prefer `itertools.pairwise()` over `zip()` when iterating over successive pairs"
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace `zip()` with `itertools.pairwise()`".to_string())
    }
}

#[derive(Debug)]
struct SliceInfo {
    id: String,
    slice_start: Option<i32>,
}

/// Return the argument name, lower bound, and upper bound for an expression, if it's a slice.
fn match_slice_info(expr: &Expr) -> Option<SliceInfo> {
    let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr else {
        return None;
    };

    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return None;
    };

    let Expr::Slice(ast::ExprSlice { lower, step, .. }) = slice.as_ref() else {
        return None;
    };

    // Avoid false positives for slices with a step.
    if let Some(step) = step {
        if !matches!(
            step.as_ref(),
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(Int::ONE),
                ..
            })
        ) {
            return None;
        }
    }

    // If the slice start is a non-constant, we can't be sure that it's successive.
    let slice_start = if let Some(lower) = lower.as_ref() {
        let Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(int),
            range: _,
        }) = lower.as_ref()
        else {
            return None;
        };
        Some(int.as_i32()?)
    } else {
        None
    };

    Some(SliceInfo {
        id: id.to_string(),
        slice_start,
    })
}

/// RUF007
pub(crate) fn zip_instead_of_pairwise(checker: &Checker, call: &ast::ExprCall) {
    let ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    } = call;

    // Require exactly two positional arguments.
    let [first, second] = args.as_ref() else {
        return;
    };

    // Require second argument to be a `Subscript`.
    if !second.is_subscript_expr() {
        return;
    }

    // Require the function to be the builtin `zip`.
    if !checker.semantic().match_builtin_expr(func, "zip") {
        return;
    }

    // Allow the first argument to be a `Name` or `Subscript`.
    let Some(first_arg_info) = ({
        if let Expr::Name(ast::ExprName { id, .. }) = first {
            Some(SliceInfo {
                id: id.to_string(),
                slice_start: None,
            })
        } else {
            match_slice_info(first)
        }
    }) else {
        return;
    };

    let Some(second_arg_info) = match_slice_info(second) else {
        return;
    };

    // Verify that the arguments match the same name.
    if first_arg_info.id != second_arg_info.id {
        return;
    }

    // Verify that the arguments are successive.
    if second_arg_info.slice_start.unwrap_or(0) - first_arg_info.slice_start.unwrap_or(0) != 1 {
        return;
    }

    let mut diagnostic = Diagnostic::new(ZipInsteadOfPairwise, func.range());

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("itertools", "pairwise"),
            func.start(),
            checker.semantic(),
        )?;
        let reference_edit =
            Edit::range_replacement(format!("{binding}({})", first_arg_info.id), call.range());
        Ok(Fix::unsafe_edits(import_edit, [reference_edit]))
    });

    checker.report_diagnostic(diagnostic);
}
