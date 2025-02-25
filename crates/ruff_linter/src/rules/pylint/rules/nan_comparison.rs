use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for comparisons against NaN values.
///
/// ## Why is this bad?
/// Comparing against a NaN value can lead to unexpected results. For example,
/// `float("NaN") == float("NaN")` will return `False` and, in general,
/// `x == float("NaN")` will always return `False`, even if `x` is `NaN`.
///
/// To determine whether a value is `NaN`, use `math.isnan` or `np.isnan`
/// instead of comparing against `NaN` directly.
///
/// ## Example
/// ```python
/// if x == float("NaN"):
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
#[derive(ViolationMetadata)]
pub(crate) struct NanComparison {
    nan: Nan,
}

impl Violation for NanComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.nan {
            Nan::Math => "Comparing against a NaN value; use `math.isnan` instead".to_string(),
            Nan::NumPy => "Comparing against a NaN value; use `np.isnan` instead".to_string(),
        }
    }
}

/// PLW0177
pub(crate) fn nan_comparison(checker: &Checker, left: &Expr, comparators: &[Expr]) {
    for expr in std::iter::once(left).chain(comparators) {
        if expr.is_call_expr() {
            if is_nan_float(expr, checker.semantic()) {
                checker.report_diagnostic(Diagnostic::new(
                    NanComparison { nan: Nan::Math },
                    expr.range(),
                ));
            }
            continue;
        }

        if let Some(nan) = Nan::from(expr, checker.semantic()) {
            checker.report_diagnostic(Diagnostic::new(NanComparison { nan }, expr.range()));
        }
    }
}

/// PLW0177
pub(crate) fn nan_comparison_match(checker: &Checker, cases: &[ast::MatchCase]) {
    for case in cases {
        let ast::Pattern::MatchValue(pattern) = &case.pattern else {
            continue;
        };

        if let Some(nan) = Nan::from(&pattern.value, checker.semantic()) {
            checker.report_diagnostic(Diagnostic::new(NanComparison { nan }, pattern.range));
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Nan {
    /// `math.isnan`
    Math,
    /// `np.isnan`
    NumPy,
}

impl Nan {
    fn from(expr: &Expr, semantic: &SemanticModel) -> Option<Self> {
        let qualified_name = semantic.resolve_qualified_name(expr)?;

        match qualified_name.segments() {
            ["numpy", "nan" | "NAN" | "NaN"] => Some(Self::NumPy),
            ["math", "nan"] => Some(Self::Math),
            _ => None,
        }
    }
}

impl std::fmt::Display for Nan {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Nan::Math => fmt.write_str("math"),
            Nan::NumPy => fmt.write_str("numpy"),
        }
    }
}

/// Returns `true` if the expression is a call to `float("NaN")`.
fn is_nan_float(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    }) = expr
    else {
        return false;
    };

    if !keywords.is_empty() {
        return false;
    }

    let [Expr::StringLiteral(ast::ExprStringLiteral { value, .. })] = &**args else {
        return false;
    };

    if !matches!(
        value.to_str(),
        "nan" | "NaN" | "NAN" | "Nan" | "nAn" | "naN" | "nAN" | "NAn"
    ) {
        return false;
    }

    semantic.match_builtin_expr(func, "float")
}
