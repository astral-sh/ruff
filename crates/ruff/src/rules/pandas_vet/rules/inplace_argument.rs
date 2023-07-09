use ruff_text_size::TextRange;
use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::source_code::Locator;
use ruff_python_semantic::{BindingKind, Import};

use crate::autofix::edits::remove_argument;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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
/// - [_Why You Should Probably Never Use pandas inplace=True_](https://towardsdatascience.com/why-you-should-probably-never-use-pandas-inplace-true-9f9f211849e4)
#[violation]
pub struct PandasUseOfInplaceArgument;

impl Violation for PandasUseOfInplaceArgument {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`inplace=True` should be avoided; it has inconsistent behavior")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Assign to variable; remove `inplace` arg".to_string())
    }
}

/// PD002
pub(crate) fn inplace_argument(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    // If the function was imported from another module, and it's _not_ Pandas, abort.
    if let Some(call_path) = checker.semantic().resolve_call_path(func) {
        if !call_path
            .first()
            .and_then(|module| checker.semantic().find_binding(module))
            .map_or(false, |binding| {
                matches!(
                    binding.kind,
                    BindingKind::Import(Import {
                        qualified_name: "pandas"
                    })
                )
            })
        {
            return;
        }
    }

    let mut seen_star = false;
    for keyword in keywords.iter().rev() {
        let Some(arg) = &keyword.arg else {
            seen_star = true;
            continue;
        };
        if arg == "inplace" {
            if is_const_true(&keyword.value) {
                let mut diagnostic = Diagnostic::new(PandasUseOfInplaceArgument, keyword.range());
                if checker.patch(diagnostic.kind.rule()) {
                    // Avoid applying the fix if:
                    // 1. The keyword argument is followed by a star argument (we can't be certain that
                    //    the star argument _doesn't_ contain an override).
                    // 2. The call is part of a larger expression (we're converting an expression to a
                    //    statement, and expressions can't contain statements).
                    // 3. The call is in a lambda (we can't assign to a variable in a lambda). This
                    //    should be unnecessary, as lambdas are expressions, and so (2) should apply,
                    //    but we don't currently restore expression stacks when parsing deferred nodes,
                    //    and so the parent is lost.
                    if !seen_star
                        && checker.semantic().stmt().is_expr_stmt()
                        && checker.semantic().expr_parent().is_none()
                        && !checker.semantic().scope().kind.is_lambda()
                    {
                        if let Some(fix) = convert_inplace_argument_to_assignment(
                            checker.locator,
                            expr,
                            keyword.range(),
                            args,
                            keywords,
                        ) {
                            diagnostic.set_fix(fix);
                        }
                    }
                }

                checker.diagnostics.push(diagnostic);
            }

            // Duplicate keywords is a syntax error, so we can stop here.
            break;
        }
    }
}

/// Remove the `inplace` argument from a function call and replace it with an
/// assignment.
fn convert_inplace_argument_to_assignment(
    locator: &Locator,
    expr: &Expr,
    expr_range: TextRange,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    // Add the assignment.
    let call = expr.as_call_expr()?;
    let attr = call.func.as_attribute_expr()?;
    let insert_assignment = Edit::insertion(
        format!("{name} = ", name = locator.slice(attr.value.range())),
        expr.start(),
    );

    // Remove the `inplace` argument.
    let remove_argument =
        remove_argument(locator, call.func.end(), expr_range, args, keywords, false).ok()?;

    Some(Fix::suggested_edits(insert_assignment, [remove_argument]))
}
