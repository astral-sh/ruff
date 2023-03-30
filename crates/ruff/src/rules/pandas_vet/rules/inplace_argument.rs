use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, StmtKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pandas_vet::fixes::convert_inplace_argument_to_assignment;

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
pub struct PandasUseOfInplaceArgument {
    pub fixable: bool,
}

impl Violation for PandasUseOfInplaceArgument {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`inplace=True` should be avoided; it has inconsistent behavior")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|_| format!("Assign to variable; remove `inplace` arg"))
    }
}

/// PD002
pub fn inplace_argument(
    checker: &Checker,
    expr: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    let mut seen_star = false;
    for keyword in keywords.iter().rev() {
        let Some(arg) = &keyword.node.arg else {
            seen_star = true;
            continue;
        };
        if arg == "inplace" {
            let is_true_literal = match &keyword.node.value.node {
                ExprKind::Constant {
                    value: Constant::Bool(boolean),
                    ..
                } => *boolean,
                _ => false,
            };
            if is_true_literal {
                // Avoid applying the fix if:
                // 1. The keyword argument is followed by a star argument (we can't be certain that
                //    the star argument _doesn't_ contain an override).
                // 2. The call is part of a larger expression (we're converting an expression to a
                //    statement, and expressions can't contain statements).
                let fixable = !seen_star
                    && matches!(checker.ctx.current_stmt().node, StmtKind::Expr { .. })
                    && checker.ctx.current_expr_parent().is_none();
                let mut diagnostic =
                    Diagnostic::new(PandasUseOfInplaceArgument { fixable }, Range::from(keyword));
                if fixable && checker.patch(diagnostic.kind.rule()) {
                    if let Some(fix) = convert_inplace_argument_to_assignment(
                        checker.locator,
                        expr,
                        diagnostic.location,
                        diagnostic.end_location,
                        args,
                        keywords,
                    ) {
                        diagnostic.set_fix(fix);
                    }
                }
                return Some(diagnostic);
            }

            // Duplicate keywords is a syntax error, so we can stop here.
            break;
        }
    }
    None
}
