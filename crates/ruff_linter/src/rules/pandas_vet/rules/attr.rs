use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{Modules, analyze::typing::find_binding_value};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::pandas_vet::helpers::{Resolution, test_expression};

/// ## What it does
/// Checks for uses of `.values` on Pandas Series and Index objects.
///
/// ## Why is this bad?
/// The `.values` attribute is ambiguous as its return type is unclear. As
/// such, it is no longer recommended by the Pandas documentation.
///
/// Instead, use `.to_numpy()` to return a NumPy array, or `.array` to return a
/// Pandas `ExtensionArray`.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// animals = pd.read_csv("animals.csv").values  # Ambiguous.
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// animals = pd.read_csv("animals.csv").to_numpy()  # Explicit.
/// ```
///
/// ## References
/// - [Pandas documentation: Accessing the values in a Series or Index](https://pandas.pydata.org/pandas-docs/stable/whatsnew/v0.24.0.html#accessing-the-values-in-a-series-or-index)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfDotValues;

impl Violation for PandasUseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `.to_numpy()` instead of `.values`".to_string()
    }
}

/// Check if a binding comes from a NumPy function that returns a `NamedTuple` with a `.values` field.
fn is_numpy_namedtuple_binding(
    expr: &Expr,
    semantic: &ruff_python_semantic::SemanticModel,
) -> bool {
    let Expr::Name(name) = expr else {
        return false;
    };

    let Some(binding_id) = semantic.resolve_name(name) else {
        return false;
    };
    let binding = semantic.binding(binding_id);

    let Some(assigned_value) = find_binding_value(binding, semantic) else {
        return false;
    };

    let Some(call_expr) = assigned_value.as_call_expr() else {
        return false;
    };

    let Some(qualified_name) = semantic.resolve_qualified_name(&call_expr.func) else {
        return false;
    };

    matches!(
        qualified_name.segments(),
        ["numpy", "unique_inverse" | "unique_all" | "unique_counts"]
    )
}

/// PD011
pub(crate) fn attr(checker: &Checker, attribute: &ast::ExprAttribute) {
    if !checker.semantic().seen_module(Modules::PANDAS) {
        return;
    }

    // Avoid, e.g., `x.values = y`.
    if !attribute.ctx.is_load() {
        return;
    }

    // Check for, e.g., `df.values`.
    if attribute.attr.as_str() != "values" {
        return;
    }

    // Avoid flagging on function calls (e.g., `df.values()`).
    if checker
        .semantic()
        .current_expression_parent()
        .is_some_and(Expr::is_call_expr)
    {
        return;
    }

    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`), and on irrelevant bindings
    // (like imports).
    if !matches!(
        test_expression(attribute.value.as_ref(), checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    // Avoid flagging on NumPy `NamedTuples` that have a legitimate `.values` field
    if is_numpy_namedtuple_binding(attribute.value.as_ref(), checker.semantic()) {
        return;
    }

    checker.report_diagnostic(PandasUseOfDotValues, attribute.range());
}
