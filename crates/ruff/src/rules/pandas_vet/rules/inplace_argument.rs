use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pandas_vet::fixes::fix_inplace_argument;

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

impl AlwaysAutofixableViolation for PandasUseOfInplaceArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`inplace=True` should be avoided; it has inconsistent behavior")
    }

    fn autofix_title(&self) -> String {
        format!("Assign to variable; remove `inplace` arg")
    }
}

/// PD002
pub fn inplace_argument(
    checker: &Checker,
    expr: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    for keyword in keywords {
        let arg = keyword.node.arg.as_ref()?;

        if arg == "inplace" {
            let is_true_literal = match &keyword.node.value.node {
                ExprKind::Constant {
                    value: Constant::Bool(boolean),
                    ..
                } => *boolean,
                _ => false,
            };
            if is_true_literal {
                let mut diagnostic =
                    Diagnostic::new(PandasUseOfInplaceArgument, Range::from(keyword));
                if checker.patch(diagnostic.kind.rule()) {
                    if let Some(fix) = fix_inplace_argument(
                        checker.locator,
                        expr,
                        diagnostic.location,
                        diagnostic.end_location,
                        args,
                        keywords,
                    ) {
                        diagnostic.amend(fix);
                    }
                }
                return Some(diagnostic);
            }
        }
    }
    None
}
