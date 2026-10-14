use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for uses of pandas APIs that were removed in pandas 3.0.
///
/// ## Why is this bad?
/// pandas 3.0 removed many functions, arguments and aliases that were
/// deprecated in earlier releases. Code that still uses them fails with an
/// error after upgrading to pandas 3.0.
///
/// This rule only flags APIs that can be resolved through a `pandas` import,
/// such as `pd.value_counts`. It does not flag methods called on `DataFrame` or
/// `Series` objects, since those cannot be detected reliably without type
/// information.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// counts = pd.value_counts(values)
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// counts = pd.Series(values).value_counts()
/// ```
///
/// ## References
/// - [pandas 3.0.0 release notes: Removal of prior version deprecations/changes](https://pandas.pydata.org/docs/whatsnew/v3.0.0.html#whatsnew-300-prior-deprecations)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct Pandas3Deprecation {
    existing: String,
    guideline: Option<String>,
}

impl Violation for Pandas3Deprecation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Pandas3Deprecation {
            existing,
            guideline,
        } = self;
        match guideline {
            Some(guideline) => format!("`{existing}` was removed in pandas 3.0. {guideline}"),
            None => format!("`{existing}` was removed in pandas 3.0"),
        }
    }
}

/// `pandas3-deprecation`: removed functions, classes and aliases.
pub(crate) fn pandas3_deprecation(checker: &Checker, expr: &Expr) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::PANDAS) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let (existing, guideline) = match qualified_name.segments() {
        ["pandas", "value_counts"] => (
            "pd.value_counts",
            Some("Use `pd.Series(...).value_counts()` instead."),
        ),
        ["pandas", "read_gbq"] => (
            "pd.read_gbq",
            Some("Use `pandas_gbq.read_gbq` from the `pandas-gbq` package instead."),
        ),
        ["pandas", "arrays", "PandasArray"] => (
            "pd.arrays.PandasArray",
            Some("Use `pd.arrays.NumpyExtensionArray` instead."),
        ),
        ["pandas", "api", "types", "is_interval"] => (
            "pd.api.types.is_interval",
            Some("Use `isinstance(obj, pd.Interval)` instead."),
        ),
        ["pandas", "api", "types", "is_period"] => (
            "pd.api.types.is_period",
            Some("Use `isinstance(obj, pd.Period)` instead."),
        ),
        ["pandas", "io", "sql", "execute"] => (
            "pandas.io.sql.execute",
            Some("Execute the query on the database connection directly."),
        ),
        _ => return,
    };

    checker.report_diagnostic(
        Pandas3Deprecation {
            existing: existing.to_string(),
            guideline: guideline.map(ToString::to_string),
        },
        expr.range(),
    );
}

/// `pandas3-deprecation`: removed keyword arguments and argument values.
pub(crate) fn pandas3_deprecated_call(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::PANDAS) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
        return;
    };

    let report = |existing: String, guideline: &str, range: ruff_text_size::TextRange| {
        checker.report_diagnostic(
            Pandas3Deprecation {
                existing,
                guideline: Some(guideline.to_string()),
            },
            range,
        );
    };

    // Report each keyword in `removed` that is passed to the call.
    let removed_keywords = |function: &str, removed: &[(&str, &str)]| {
        for (name, guideline) in removed {
            if let Some(keyword) = call.arguments.find_keyword(name) {
                report(
                    format!("{function}({name}=...)"),
                    guideline,
                    keyword.range(),
                );
            }
        }
    };

    match qualified_name.segments() {
        ["pandas", function @ ("read_csv" | "read_table")] => {
            removed_keywords(
                &format!("pd.{function}"),
                &[
                    ("delim_whitespace", "Use `sep=r\"\\s+\"` instead."),
                    ("verbose", "Remove the argument."),
                    (
                        "infer_datetime_format",
                        "Remove the argument; strict datetime format inference is now the default.",
                    ),
                    (
                        "keep_date_col",
                        "Remove the argument and combine the date columns after reading.",
                    ),
                    ("date_parser", "Use `date_format` instead."),
                ],
            );
        }
        ["pandas", function @ ("read_fwf" | "read_excel")] => {
            removed_keywords(
                &format!("pd.{function}"),
                &[("date_parser", "Use `date_format` instead.")],
            );
        }
        ["pandas", "read_parquet"] => {
            removed_keywords(
                "pd.read_parquet",
                &[(
                    "use_nullable_dtypes",
                    "Use `dtype_backend=\"numpy_nullable\"` instead.",
                )],
            );
        }
        [
            "pandas",
            function @ ("to_datetime" | "to_numeric" | "to_timedelta"),
        ] => {
            if let Some(keyword) = call.arguments.find_keyword("errors") {
                if is_string_literal(&keyword.value, "ignore") {
                    report(
                        format!("pd.{function}(errors=\"ignore\")"),
                        "Catch the exception explicitly instead.",
                        keyword.range(),
                    );
                }
            }
        }
        ["pandas", "set_option"] => {
            if let Some(Expr::StringLiteral(option)) = call.arguments.find_positional(0) {
                if option.value.to_str().ends_with("use_inf_as_na") {
                    report(
                        "pd.set_option(\"mode.use_inf_as_na\", ...)".to_string(),
                        "Convert `inf` values to `NaN` explicitly instead.",
                        option.range(),
                    );
                }
            }
        }
        ["pandas", "PeriodIndex"] => {
            removed_keywords(
                "pd.PeriodIndex",
                &[
                    ("year", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("quarter", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("month", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("day", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("hour", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("minute", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("second", "Use `pd.PeriodIndex.from_fields` instead."),
                    ("ordinal", "Use `pd.PeriodIndex.from_ordinals` instead."),
                ],
            );
        }
        ["pandas", "DatetimeIndex"] => {
            removed_keywords(
                "pd.DatetimeIndex",
                &[
                    ("closed", "Remove the argument."),
                    ("normalize", "Call `.normalize()` on the result instead."),
                ],
            );
        }
        ["pandas", "TimedeltaIndex"] => {
            removed_keywords(
                "pd.TimedeltaIndex",
                &[
                    ("closed", "Remove the argument."),
                    ("unit", "Use `pd.to_timedelta(..., unit=...)` instead."),
                ],
            );
        }
        ["pandas", function @ ("Series" | "Categorical")] => {
            removed_keywords(
                &format!("pd.{function}"),
                &[("fastpath", "Remove the argument.")],
            );
        }
        ["pandas", "StringDtype"] => {
            if let Some(storage) = call.arguments.find_argument_value("storage", 0) {
                if is_string_literal(storage, "pyarrow_numpy") {
                    report(
                        "pd.StringDtype(\"pyarrow_numpy\")".to_string(),
                        "Use `pd.StringDtype(\"pyarrow\", na_value=np.nan)` instead.",
                        storage.range(),
                    );
                }
            }
        }
        _ => {}
    }
}

/// Returns `true` if `expr` is the string literal `value`.
fn is_string_literal(expr: &Expr, value: &str) -> bool {
    matches!(expr, Expr::StringLiteral(literal) if literal.value.to_str() == value)
}
