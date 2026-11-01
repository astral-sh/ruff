use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprCall};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::helpers::parametrize_has_value_rows;

/// ## What it does
/// Checks for non-literal values in `pytest.mark.parametrize` that can generate
/// unstable test IDs.
///
/// ## Why is this bad?
/// pytest derives test IDs from parameter values when an explicit ID is not
/// provided. For dynamically generated values, such as UUID strings, the
/// generated ID can change each time tests are collected.
///
/// Unstable test IDs make it difficult to rerun individual tests and can cause
/// collection failures when tests are distributed across multiple workers.
///
/// Provide a stable ID with `pytest.param(..., id=...)` or the `ids` argument
/// to `pytest.mark.parametrize`.
///
/// ## Example
/// ```python
/// from uuid import uuid4
///
/// import pytest
///
///
/// @pytest.mark.parametrize("value", [str(uuid4()), "invalid"])
/// def test_value(value): ...
/// ```
///
/// Use instead:
/// ```python
/// from uuid import uuid4
///
/// import pytest
///
///
/// @pytest.mark.parametrize(
///     "value",
///     [pytest.param(str(uuid4()), id="valid-uuid"), "invalid"],
/// )
/// def test_value(value): ...
/// ```
///
/// ## References
/// - [`pytest` documentation: Different options for test IDs](https://docs.pytest.org/en/stable/example/parametrize.html#different-options-for-test-ids)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct PytestParametrizeUnstableId;

impl Violation for PytestParametrizeUnstableId {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Non-literal parameter value in `pytest.mark.parametrize` may generate an unstable test ID"
            .to_string()
    }
}

/// PT901
pub(crate) fn pytest_parametrize_unstable_id(
    checker: &Checker,
    call: &ExprCall,
    names: &Expr,
    values: &Expr,
) {
    if call.arguments.any_variadic() {
        return;
    }

    let (Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })) =
        values
    else {
        return;
    };

    if elts.iter().any(Expr::is_starred_expr) {
        return;
    }

    let Some(has_value_rows) = parametrize_has_value_rows(names) else {
        return;
    };

    let ids = match call.arguments.find_argument_value("ids", 3) {
        Some(Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })) => {
            if elts.iter().any(Expr::is_starred_expr) {
                return;
            }

            Some(elts.as_slice())
        }
        Some(id) if !id.is_none_literal_expr() => return,
        Some(_) | None => None,
    };

    let ids = ids
        .unwrap_or(&[])
        .iter()
        .map(Some)
        .chain(std::iter::repeat(None));

    for (value, id) in elts.iter().zip(ids) {
        if id.is_some_and(|id| !id.is_none_literal_expr()) {
            continue;
        }

        if let Some(non_literal) = unstable_parameter_value(checker, value, has_value_rows) {
            checker.report_diagnostic(PytestParametrizeUnstableId, non_literal.range());
        }
    }
}

fn unstable_parameter_value<'a>(
    checker: &Checker,
    value: &'a Expr,
    has_value_rows: bool,
) -> Option<&'a Expr> {
    match value {
        Expr::Call(param)
            if checker
                .semantic()
                .resolve_qualified_name(&param.func)
                .is_some_and(|name| matches!(name.segments(), ["pytest", "param"])) =>
        {
            if param.arguments.any_variadic()
                || param
                    .arguments
                    .find_keyword("id")
                    .is_some_and(|id| !id.value.is_none_literal_expr())
            {
                return None;
            }

            param
                .arguments
                .args
                .iter()
                .find(|argument| !has_stable_default_id(argument))
        }
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. })
            if has_value_rows =>
        {
            if elts.iter().any(Expr::is_starred_expr) {
                return None;
            }

            elts.iter()
                .find(|argument| !has_stable_default_id(argument))
        }
        value if !has_stable_default_id(value) => Some(value),
        _ => None,
    }
}

fn has_stable_default_id(value: &Expr) -> bool {
    value.is_literal_expr()
        || matches!(
            value,
            Expr::List(_)
                | Expr::Tuple(_)
                | Expr::Set(_)
                | Expr::Dict(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
        )
        || matches!(
            value,
            Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) if operand.is_number_literal_expr()
        )
}
