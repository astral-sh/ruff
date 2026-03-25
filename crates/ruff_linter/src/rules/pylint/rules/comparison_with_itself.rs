use itertools::Itertools;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::preview::is_comparison_with_itself_extended;

/// ## What it does
/// Checks for operations that compare an expression to itself.
///
/// ## Why is this bad?
/// Comparing an expression to itself always results in the same value, and is
/// likely a mistake.
///
/// ## Example
/// ```python
/// foo == foo
/// ```
///
/// In [preview], this rule also detects self-comparisons involving attribute
/// accesses, subscripts, and function calls:
/// ```python
/// self.x == self.x
/// a[0] == a[0]
/// ```
///
/// In some cases, self-comparisons are used to determine whether a float is
/// NaN. Instead, prefer `math.isnan`:
/// ```python
/// import math
///
/// math.isnan(foo)
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.273")]
pub(crate) struct ComparisonWithItself {
    actual: SourceCodeSnippet,
}

impl Violation for ComparisonWithItself {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(actual) = self.actual.full_display() {
            format!("Name compared with itself, consider replacing `{actual}`")
        } else {
            "Name compared with itself".to_string()
        }
    }
}

/// PLR0124
pub(crate) fn comparison_with_itself(
    checker: &Checker,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    for ((left, right), op) in std::iter::once(left)
        .chain(comparators)
        .tuple_windows()
        .zip(ops)
    {
        // Ex) `foo == foo`
        if let (Expr::Name(left_name), Expr::Name(right_name)) = (left, right)
            && left_name.id == right_name.id
        {
            let actual = format!(
                "{} {} {}",
                checker.locator().slice(left),
                op,
                checker.locator().slice(right)
            );
            checker.report_diagnostic(
                ComparisonWithItself {
                    actual: SourceCodeSnippet::new(actual),
                },
                left_name.range(),
            );
            continue;
        }

        // Ex) `id(foo) == id(foo)` (stable: only builtin pure functions)
        if let (Expr::Call(left_call), Expr::Call(right_call)) = (left, right)
            && is_builtin_self_comparison(checker, left_call, right_call)
        {
            let actual = format!(
                "{} {} {}",
                checker.locator().slice(left),
                op,
                checker.locator().slice(right)
            );
            checker.report_diagnostic(
                ComparisonWithItself {
                    actual: SourceCodeSnippet::new(actual),
                },
                left_call.range(),
            );
            continue;
        }

        // Ex) `self.x == self.x`, `a[0] == a[0]`, `obj.method() == obj.method()`
        if is_comparison_with_itself_extended(checker.settings())
            && !left.is_name_expr()
            && !left.is_literal_expr()
            && ComparableExpr::from(left) == ComparableExpr::from(right)
        {
            let actual = format!(
                "{} {} {}",
                checker.locator().slice(left),
                op,
                checker.locator().slice(right)
            );
            checker.report_diagnostic(
                ComparisonWithItself {
                    actual: SourceCodeSnippet::new(actual),
                },
                left.range(),
            );
        }
    }
}

/// Returns `true` if the two calls are to the same builtin pure function with
/// the same single argument (e.g., `id(foo) == id(foo)`).
fn is_builtin_self_comparison(
    checker: &Checker,
    left_call: &ruff_python_ast::ExprCall,
    right_call: &ruff_python_ast::ExprCall,
) -> bool {
    if !left_call.arguments.keywords.is_empty() || !right_call.arguments.keywords.is_empty() {
        return false;
    }
    let [Expr::Name(left_arg)] = &*left_call.arguments.args else {
        return false;
    };
    let [Expr::Name(right_arg)] = &*right_call.arguments.args else {
        return false;
    };
    if left_arg.id != right_arg.id {
        return false;
    }

    let semantic = checker.semantic();
    let Some(left_name) = semantic.resolve_builtin_symbol(&left_call.func) else {
        return false;
    };
    let Some(right_name) = semantic.resolve_builtin_symbol(&right_call.func) else {
        return false;
    };
    if left_name != right_name {
        return false;
    }

    matches!(
        left_name,
        "id" | "len" | "type" | "int" | "bool" | "str" | "repr" | "bytes"
    )
}
