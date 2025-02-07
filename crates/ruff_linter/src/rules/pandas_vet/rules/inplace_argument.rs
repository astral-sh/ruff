use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Keyword, Stmt};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::Locator;

/// ## What it does
/// Checks for `inplace=True` usages in `pandas` function and method
/// calls.
///
/// ## Why is this bad?
/// Using `inplace=True` encourages mutation rather than immutable data,
/// which is harder to reason about and may cause bugs. It also removes the
/// ability to use the method chaining style for `pandas` operations.
///
/// Further, in many cases, `inplace=True` does not provide a performance
/// benefit, as `pandas` will often copy `DataFrames` in the background.
///
/// ## Example
/// ```python
/// df.sort_values("col1", inplace=True)
/// ```
///
/// Use instead:
/// ```python
/// sorted_df = df.sort_values("col1")
/// ```
///
/// ## References
/// - [_Why You Should Probably Never Use pandas `inplace=True`_](https://towardsdatascience.com/why-you-should-probably-never-use-pandas-inplace-true-9f9f211849e4)
#[derive(ViolationMetadata)]
pub(crate) struct PandasUseOfInplaceArgument;

impl Violation for PandasUseOfInplaceArgument {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`inplace=True` should be avoided; it has inconsistent behavior".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Assign to variable; remove `inplace` arg".to_string())
    }
}

/// PD002
pub(crate) fn inplace_argument(checker: &Checker, call: &ast::ExprCall) {
    // If the function was imported from another module, and it's _not_ Pandas, abort.
    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| !matches!(qualified_name.segments(), ["pandas", ..]))
    {
        return;
    }

    // If the function doesn't take an `inplace` argument, abort.
    if !call
        .func
        .as_attribute_expr()
        .is_some_and(|func| accepts_inplace_argument(&func.attr))
    {
        return;
    }

    let mut seen_star = false;
    for keyword in call.arguments.keywords.iter().rev() {
        let Some(arg) = &keyword.arg else {
            seen_star = true;
            continue;
        };
        if arg == "inplace" {
            if is_const_true(&keyword.value) {
                let mut diagnostic = Diagnostic::new(PandasUseOfInplaceArgument, keyword.range());
                // Avoid applying the fix if:
                // 1. The keyword argument is followed by a star argument (we can't be certain that
                //    the star argument _doesn't_ contain an override).
                // 2. The call is part of a larger expression (we're converting an expression to a
                //    statement, and expressions can't contain statements).
                let statement = checker.semantic().current_statement();
                if !seen_star
                    && checker.semantic().current_expression_parent().is_none()
                    && statement.is_expr_stmt()
                {
                    if let Some(fix) = convert_inplace_argument_to_assignment(
                        call,
                        keyword,
                        statement,
                        checker.comment_ranges(),
                        checker.locator(),
                    ) {
                        diagnostic.set_fix(fix);
                    }
                }

                checker.report_diagnostic(diagnostic);
            }

            // Duplicate keywords is a syntax error, so we can stop here.
            break;
        }
    }
}

/// Remove the `inplace` argument from a function call and replace it with an
/// assignment.
fn convert_inplace_argument_to_assignment(
    call: &ast::ExprCall,
    keyword: &Keyword,
    statement: &Stmt,
    comment_ranges: &CommentRanges,
    locator: &Locator,
) -> Option<Fix> {
    // Add the assignment.
    let attr = call.func.as_attribute_expr()?;
    let insert_assignment = Edit::insertion(
        format!("{name} = ", name = locator.slice(attr.value.range())),
        parenthesized_range(
            call.into(),
            statement.into(),
            comment_ranges,
            locator.contents(),
        )
        .unwrap_or(call.range())
        .start(),
    );

    // Remove the `inplace` argument.
    let remove_argument = remove_argument(
        keyword,
        &call.arguments,
        Parentheses::Preserve,
        locator.contents(),
    )
    .ok()?;

    Some(Fix::unsafe_edits(insert_assignment, [remove_argument]))
}

/// Returns `true` if the given method accepts an `inplace` argument when used on a Pandas
/// `DataFrame`, `Series`, or `Index`.
///
/// See: <https://pandas.pydata.org/docs/reference/frame.html>
fn accepts_inplace_argument(method: &str) -> bool {
    matches!(
        method,
        "where"
            | "mask"
            | "query"
            | "clip"
            | "eval"
            | "backfill"
            | "bfill"
            | "ffill"
            | "fillna"
            | "interpolate"
            | "dropna"
            | "pad"
            | "replace"
            | "drop"
            | "drop_duplicates"
            | "rename"
            | "rename_axis"
            | "reset_index"
            | "set_index"
            | "sort_values"
            | "sort_index"
            | "set_names"
    )
}
