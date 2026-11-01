use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::{Parentheses, remove_argument};
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for uses of pandas APIs that were removed in pandas 3.0.
///
/// This includes:
/// - Removed functions and aliases, such as `pd.value_counts`, `pd.read_gbq` and
///   `pd.arrays.PandasArray`.
/// - Removed keyword arguments, such as `pd.read_csv(delim_whitespace=...)`,
///   `pd.read_parquet(use_nullable_dtypes=...)` and `pd.PeriodIndex(year=...)`.
/// - Removed argument values, such as `pd.to_datetime(errors="ignore")` and
///   `pd.StringDtype("pyarrow_numpy")`.
/// - Removed frequency aliases, such as `freq="M"` in `pd.date_range` (now `"ME"`) and
///   `freq="H"` (now `"h"`), and removed `pd.Timedelta` units, such as `unit="T"`
///   (now `"min"`).
///
/// ## Why is this bad?
/// pandas 3.0 removed many functions, arguments and aliases that were
/// deprecated in earlier releases. Code that still uses them fails with an
/// error after upgrading to pandas 3.0.
///
/// ## Known problems
/// This rule only flags APIs that can be resolved through a `pandas` import,
/// such as `pd.value_counts`. It does not flag methods called on `DataFrame` or
/// `Series` objects, such as `df.applymap`, since those cannot be detected
/// reliably without type information.
///
/// Frequency aliases and units are only checked when they are passed as string
/// literals. Period APIs such as `pd.period_range` are not checked, since `"M"`,
/// `"Q"` and `"Y"` are still valid period frequencies.
///
/// ## Example
/// ```python
/// import pandas as pd
///
/// counts = pd.value_counts(values)
/// df = pd.read_csv("data.txt", delim_whitespace=True)
/// index = pd.date_range("2024-01-01", periods=12, freq="M")
/// ```
///
/// Use instead:
/// ```python
/// import pandas as pd
///
/// counts = pd.Series(values).value_counts()
/// df = pd.read_csv("data.txt", sep=r"\s+")
/// index = pd.date_range("2024-01-01", periods=12, freq="ME")
/// ```
///
/// ## Fix safety
/// Replacing `delim_whitespace=True` with `sep=r"\s+"` is safe, since it behaves the
/// same on all pandas versions. The fix is only offered when no separator is passed.
///
/// Removing `verbose=...` and `fastpath=...` is safe, since these arguments don't
/// change the result.
///
/// Removing `infer_datetime_format=...` is unsafe, since it can change how dates are
/// parsed on pandas versions older than 2.0. Replacing `use_nullable_dtypes=True`
/// with `dtype_backend="numpy_nullable"` is unsafe, since `dtype_backend` was added
/// in pandas 2.0.
///
/// All fixes are marked as unsafe if the argument contains a comment, since the
/// comment would be removed.
///
/// The fixes for frequency aliases, `pd.Timedelta` units and
/// `pd.arrays.PandasArray` are unsafe, since the new spellings are not supported
/// by older pandas versions.
///
/// ## References
/// - [pandas 3.0.0 release notes: Removal of prior version deprecations/changes](https://pandas.pydata.org/docs/whatsnew/v3.0.0.html#whatsnew-300-prior-deprecations)
/// - [pandas documentation: Offset aliases](https://pandas.pydata.org/docs/user_guide/timeseries.html#offset-aliases)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct Pandas3Deprecation {
    existing: String,
    guideline: Option<String>,
    fix_title: Option<String>,
}

impl Violation for Pandas3Deprecation {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Pandas3Deprecation {
            existing,
            guideline,
            fix_title: _,
        } = self;
        match guideline {
            Some(guideline) => format!("`{existing}` was removed in pandas 3.0. {guideline}"),
            None => format!("`{existing}` was removed in pandas 3.0"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        self.fix_title.clone()
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

    // `pd.arrays.PandasArray` can be renamed in place. `NumpyExtensionArray` only exists in
    // pandas 2.1 and later, so the fix is unsafe.
    let rename = match expr {
        Expr::Attribute(attribute) if attribute.attr.as_str() == "PandasArray" => Some(attribute),
        _ => None,
    };

    let mut diagnostic = checker.report_diagnostic(
        Pandas3Deprecation {
            existing: existing.to_string(),
            guideline: guideline.map(ToString::to_string),
            fix_title: rename.map(|_| "Replace with `NumpyExtensionArray`".to_string()),
        },
        expr.range(),
    );
    if let Some(attribute) = rename {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            "NumpyExtensionArray".to_string(),
            attribute.attr.range(),
        )));
    }
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
                fix_title: None,
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
            if let Some(keyword) = call.arguments.find_keyword("delim_whitespace") {
                delim_whitespace(checker, call, keyword, function);
            }
            removed_keyword_with_fix(
                checker,
                call,
                &format!("pd.{function}"),
                "verbose",
                "Remove the argument.",
                Some(KeywordFix::Remove(Applicability::Safe)),
            );
            removed_keyword_with_fix(
                checker,
                call,
                &format!("pd.{function}"),
                "infer_datetime_format",
                "Remove the argument; strict datetime format inference is now the default.",
                Some(KeywordFix::Remove(Applicability::Unsafe)),
            );
            removed_keywords(
                &format!("pd.{function}"),
                &[
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
            let fix = match call.arguments.find_keyword("use_nullable_dtypes") {
                Some(ast::Keyword {
                    value: Expr::BooleanLiteral(literal),
                    ..
                }) if call.arguments.find_keyword("dtype_backend").is_none() => {
                    if literal.value {
                        Some(KeywordFix::Replace(
                            "dtype_backend=\"numpy_nullable\"",
                            Applicability::Unsafe,
                        ))
                    } else {
                        Some(KeywordFix::Remove(Applicability::Safe))
                    }
                }
                _ => None,
            };
            removed_keyword_with_fix(
                checker,
                call,
                "pd.read_parquet",
                "use_nullable_dtypes",
                "Use `dtype_backend=\"numpy_nullable\"` instead.",
                fix,
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
            if *function == "to_timedelta" {
                check_unit(checker, call, function);
            }
        }
        ["pandas", function @ ("set_option" | "option_context")] => {
            for argument in &*call.arguments.args {
                if let Expr::StringLiteral(option) = argument {
                    if option.value.to_str().ends_with("use_inf_as_na") {
                        report(
                            format!("pd.{function}(\"mode.use_inf_as_na\", ...)"),
                            "Convert `inf` values to `NaN` explicitly instead.",
                            option.range(),
                        );
                    }
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
            removed_keyword_with_fix(
                checker,
                call,
                &format!("pd.{function}"),
                "fastpath",
                "Remove the argument.",
                Some(KeywordFix::Remove(Applicability::Safe)),
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
        [
            "pandas",
            function @ ("date_range" | "bdate_range" | "timedelta_range"),
        ] => {
            if let Some(freq) = call.arguments.find_argument_value("freq", 3) {
                check_frequency(checker, freq, &format!("pd.{function}"), "freq");
            }
        }
        ["pandas", "Grouper"] => {
            if let Some(keyword) = call.arguments.find_keyword("freq") {
                check_frequency(checker, &keyword.value, "pd.Grouper", "freq");
            }
        }
        ["pandas", "tseries", "frequencies", "to_offset"] => {
            if let Some(freq) = call.arguments.find_argument_value("freq", 0) {
                check_frequency(checker, freq, "pd.tseries.frequencies.to_offset", "freq");
            }
        }
        ["pandas", function @ "Timedelta"] => {
            check_unit(checker, call, function);
        }
        _ => {}
    }
}

/// How to fix a removed keyword argument.
#[derive(Clone, Copy)]
enum KeywordFix {
    /// Remove the argument.
    Remove(Applicability),
    /// Replace the whole `name=value` argument with this text.
    Replace(&'static str, Applicability),
}

/// Report the keyword argument `name` if it is passed to `call`, with an optional fix.
///
/// If the argument contains a comment, the fix is downgraded to unsafe, since the comment would
/// be removed.
fn removed_keyword_with_fix(
    checker: &Checker,
    call: &ast::ExprCall,
    function: &str,
    name: &str,
    guideline: &str,
    fix: Option<KeywordFix>,
) {
    let Some(keyword) = call.arguments.find_keyword(name) else {
        return;
    };
    let fix_title = match fix {
        Some(KeywordFix::Remove(_)) => Some(format!("Remove `{name}`")),
        Some(KeywordFix::Replace(new, _)) => Some(format!("Replace with `{new}`")),
        None => None,
    };

    let mut diagnostic = checker.report_diagnostic(
        Pandas3Deprecation {
            existing: format!("{function}({name}=...)"),
            guideline: Some(guideline.to_string()),
            fix_title,
        },
        keyword.range(),
    );

    let applicability = |applicability: Applicability| {
        if checker.comment_ranges().intersects(keyword.range()) {
            Applicability::Unsafe
        } else {
            applicability
        }
    };
    match fix {
        Some(KeywordFix::Remove(base)) => {
            diagnostic.try_set_fix(|| {
                remove_argument(
                    keyword,
                    &call.arguments,
                    Parentheses::Preserve,
                    checker.locator().contents(),
                    checker.tokens(),
                )
                .map(|edit| Fix::applicable_edit(edit, applicability(base)))
            });
        }
        Some(KeywordFix::Replace(new, base)) => {
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_replacement(new.to_string(), keyword.range()),
                applicability(base),
            ));
        }
        None => {}
    }
}

/// Report `delim_whitespace=...`. `delim_whitespace=True` is replaced with `sep=r"\s+"` when no
/// separator is passed, which behaves the same on all pandas versions.
fn delim_whitespace(
    checker: &Checker,
    call: &ast::ExprCall,
    keyword: &ast::Keyword,
    function: &str,
) {
    let value = match &keyword.value {
        Expr::BooleanLiteral(literal) => Some(literal.value),
        _ => None,
    };
    let (guideline, fix_title) = if value == Some(false) {
        ("Remove the argument.", None)
    } else {
        (
            "Use `sep=r\"\\s+\"` instead.",
            Some("Replace with `sep=r\"\\s+\"`".to_string()),
        )
    };
    let has_separator = call.arguments.find_argument("sep", 1).is_some()
        || call.arguments.find_keyword("delimiter").is_some();
    let fixable = value == Some(true) && !has_separator;

    let mut diagnostic = checker.report_diagnostic(
        Pandas3Deprecation {
            existing: format!("pd.{function}(delim_whitespace=...)"),
            guideline: Some(guideline.to_string()),
            fix_title: if fixable { fix_title } else { None },
        },
        keyword.range(),
    );
    if fixable {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            r#"sep=r"\s+""#.to_string(),
            keyword.range(),
        )));
    }
}

/// Report a string literal argument whose value changed in pandas 3.0, with a fix that replaces
/// the old value with `new`.
///
/// The fix is unsafe because the new spellings are not recognized by older pandas versions.
fn report_renamed_string(
    checker: &Checker,
    literal: &ast::ExprStringLiteral,
    existing: String,
    guideline: String,
    new: &str,
) {
    let old = literal.value.to_str();
    let source = checker.locator().slice(literal.range());
    // Only fix simple literals, where the value appears exactly once in the source text.
    let edit = (!literal.value.is_implicit_concatenated() && source.matches(old).count() == 1)
        .then(|| Edit::range_replacement(source.replacen(old, new, 1), literal.range()));

    let mut diagnostic = checker.report_diagnostic(
        Pandas3Deprecation {
            existing,
            guideline: Some(guideline),
            fix_title: edit.as_ref().map(|_| format!("Replace with `\"{new}\"`")),
        },
        literal.range(),
    );
    if let Some(edit) = edit {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    }
}

/// Report `expr` if it is a string literal that uses a frequency alias removed in pandas 3.0.
fn check_frequency(checker: &Checker, expr: &Expr, function: &str, argument: &str) {
    let Expr::StringLiteral(literal) = expr else {
        return;
    };
    let value = literal.value.to_str();
    if let Some(new) = replace_frequency_alias(value) {
        report_renamed_string(
            checker,
            literal,
            format!("{function}({argument}=\"{value}\")"),
            format!("Use `{argument}=\"{new}\"` instead."),
            &new,
        );
    }
}

/// Returns the pandas 3.0 spelling of a frequency string like `"15T"` or `"Q-NOV"`, or `None` if
/// the string does not use a removed alias.
///
/// A frequency string is an optional multiple (`15`), an alias (`T`) and an optional anchor
/// (`-NOV`). Only the alias changes.
fn replace_frequency_alias(value: &str) -> Option<String> {
    let alias_start = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (multiple, rest) = value.split_at(alias_start);
    let (alias, anchor) = match rest.split_once('-') {
        Some((alias, anchor)) => (alias, Some(anchor)),
        None => (rest, None),
    };
    let new = match alias {
        "M" => "ME",
        "BM" => "BME",
        "SM" => "SME",
        "CBM" => "CBME",
        "Q" => "QE",
        "BQ" => "BQE",
        "Y" | "A" => "YE",
        "BY" | "BA" => "BYE",
        "AS" => "YS",
        "BAS" => "BYS",
        "H" => "h",
        "BH" => "bh",
        "CBH" => "cbh",
        "T" => "min",
        "L" => "ms",
        "U" => "us",
        "N" => "ns",
        _ => return None,
    };
    Some(match anchor {
        Some(anchor) => format!("{multiple}{new}-{anchor}"),
        None => format!("{multiple}{new}"),
    })
}

/// Report the `unit` argument of `pd.Timedelta` or `pd.to_timedelta` if it uses a unit removed in
/// pandas 3.0.
fn check_unit(checker: &Checker, call: &ast::ExprCall, function: &str) {
    let Some(Expr::StringLiteral(unit)) = call.arguments.find_argument_value("unit", 1) else {
        return;
    };
    let value = unit.value.to_str();
    if let Some(new) = removed_unit(value) {
        report_renamed_string(
            checker,
            unit,
            format!("pd.{function}(unit=\"{value}\")"),
            format!("Use `unit=\"{new}\"` instead."),
            new,
        );
    }
}

/// Returns the pandas 3.0 spelling of a `Timedelta` unit removed in pandas 3.0.
fn removed_unit(unit: &str) -> Option<&'static str> {
    match unit {
        "H" => Some("h"),
        "T" => Some("min"),
        "L" => Some("ms"),
        "U" => Some("us"),
        "N" => Some("ns"),
        _ => None,
    }
}

/// Returns `true` if `expr` is the string literal `value`.
fn is_string_literal(expr: &Expr, value: &str) -> bool {
    matches!(expr, Expr::StringLiteral(literal) if literal.value.to_str() == value)
}
