use rustpython_parser::ast::{self, Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{BindingKind, Importation};

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
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    let mut seen_star = false;
    let mut is_checkable = false;
    let mut is_pandas = false;

    if let Some(call_path) = checker.semantic_model().resolve_call_path(func) {
        is_checkable = true;

        let module = call_path[0];
        is_pandas = checker
            .semantic_model()
            .find_binding(module)
            .map_or(false, |binding| {
                matches!(
                    binding.kind,
                    BindingKind::Importation(Importation {
                        qualified_name: "pandas"
                    })
                )
            });
    }

    for keyword in keywords.iter().rev() {
        let Some(arg) = &keyword.arg else {
            seen_star = true;
            continue;
        };
        if arg == "inplace" {
            let is_true_literal = match &keyword.value {
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Bool(boolean),
                    ..
                }) => *boolean,
                _ => false,
            };
            if is_true_literal {
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
                        && checker.semantic_model().stmt().is_expr_stmt()
                        && checker.semantic_model().expr_parent().is_none()
                        && !checker.semantic_model().scope().kind.is_lambda()
                    {
                        if let Some(fix) = convert_inplace_argument_to_assignment(
                            checker.locator,
                            expr,
                            diagnostic.range(),
                            args,
                            keywords,
                        ) {
                            diagnostic.set_fix(fix);
                        }
                    }
                }

                // Without a static type system, only module-level functions could potentially be
                // non-pandas calls. If they're not, `inplace` should be considered safe.
                if is_checkable && !is_pandas {
                    return None;
                }

                return Some(diagnostic);
            }

            // Duplicate keywords is a syntax error, so we can stop here.
            break;
        }
    }
    None
}
