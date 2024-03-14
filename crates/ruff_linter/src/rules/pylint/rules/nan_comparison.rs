use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for comparisons against NaN values.
///
/// ## Why is this bad?
/// Comparing against a NaN value will always return False even if both values are NaN.
///
/// ## Example
/// ```python
/// if x == float('NaN'):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import math
///
/// if math.isnan(x):
///     pass
/// ```
///
#[violation]
pub struct NanComparison {
    using_numpy: bool,
}

impl Violation for NanComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NanComparison { using_numpy } = self;
        if *using_numpy {
            format!("Comparing against a NaN value, consider using `np.isnan`")
        } else {
            format!("Comparing against a NaN value, consider using `math.isnan`")
        }
    }
}

fn is_nan_float(expr: &Expr) -> bool {
    let Expr::Call(call) = expr else {
        return false;
    };

    let Expr::Name(ast::ExprName { id, .. }) = ast::helpers::map_subscript(call.func.as_ref())
    else {
        return false;
    };

    if id.as_str() != "float" {
        return false;
    }

    let Some(arg) = call.arguments.find_positional(0) else {
        return false;
    };

    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = arg {
        return value.to_str().to_lowercase() == "nan";
    }

    false
}

/// PLW0117
pub(crate) fn nan_comparison(checker: &mut Checker, left: &Expr, comparators: &[Expr]) {
    for comparison_expr in std::iter::once(left).chain(comparators.iter()) {
        if let Some(qualified_name) = checker.semantic().resolve_qualified_name(comparison_expr) {
            let segments = qualified_name.segments();
            match segments[0] {
                "numpy" => {
                    if segments[1].to_lowercase() == "nan" {
                        checker.diagnostics.push(Diagnostic::new(
                            NanComparison { using_numpy: true },
                            comparison_expr.range(),
                        ));
                    }
                }
                "math" => {
                    if segments[1] == "nan" {
                        checker.diagnostics.push(Diagnostic::new(
                            NanComparison { using_numpy: false },
                            comparison_expr.range(),
                        ));
                    }
                }
                _ => continue,
            }
        }

        if is_nan_float(comparison_expr) {
            checker.diagnostics.push(Diagnostic::new(
                NanComparison { using_numpy: false },
                comparison_expr.range(),
            ));
        }
    }
}
