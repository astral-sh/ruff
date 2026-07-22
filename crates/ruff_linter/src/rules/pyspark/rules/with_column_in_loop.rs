use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for calls to `.withColumn()` method of Spark dataframe inside of `for` or
/// `while` loops.
///
/// ## Why is this bad?
/// In Spark dataframes are immutable. Every time you call `.withColumn()`,
/// Spark creates a new `DataFrame` by generating a new logical plan that includes
/// the previous plan.
///
/// When called repeatedly inside a loop, this creates a deeply nested, massive
/// logical plan. The Spark Catalyst Optimizer must evaluate this entire chain
/// before execution. This can lead to driver-side performance degradation
/// and excessively long plan compilation times.
///
/// ## Example
/// ```python
/// new_columns = {"col_c": F.lit(3), "col_d": F.lit(4)}
///
/// for col_name, expr in new_columns.items():
///     df = df.withColumn(col_name, expr)
/// ```
///
/// Use instead:
/// Instead of looping, use `.withColumns()` (introduced in `PySpark` 3.3.0) to
/// add multiple columns at once, or use `.select()` with list comprehension.
/// ```python
/// new_columns = {"col_c": F.lit(3), "col_d": F.lit(4)}
///
/// # Pass a dictionary of expressions
/// df = df.withColumns(new_columns)
///
/// # Alternatively, using .select()
/// df = df.select("*", *[expr.alias(col_name) for col_name, expr in new_columns.items()])

#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct WithColumnInLoop;

impl Violation for WithColumnInLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Don't use `.withColumn` method in a loop".to_string()
    }
}

/// PSP001
pub(crate) fn with_column_in_loop(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::PYSPARK) {
        return;
    }

    let is_loop = checker
        .semantic()
        .current_statements()
        .any(|stmt| matches!(stmt, Stmt::For(_) | Stmt::While(_)));

    if !is_loop {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = call.func.as_ref() else {
        return;
    };

    if attr != "withColumn" {
        return;
    }
    checker.report_diagnostic(WithColumnInLoop, call.func.range());
}
