use anyhow::bail;
use itertools::Itertools;
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum EmptyStringCmpOp {
    Is,
    IsNot,
    Eq,
    NotEq,
}

impl TryFrom<&CmpOp> for EmptyStringCmpOp {
    type Error = anyhow::Error;

    fn try_from(value: &CmpOp) -> Result<Self, Self::Error> {
        match value {
            CmpOp::Is => Ok(Self::Is),
            CmpOp::IsNot => Ok(Self::IsNot),
            CmpOp::Eq => Ok(Self::Eq),
            CmpOp::NotEq => Ok(Self::NotEq),
            _ => bail!("{value:?} cannot be converted to EmptyStringCmpOp"),
        }
    }
}

impl EmptyStringCmpOp {
    pub(crate) fn into_unary(self) -> &'static str {
        match self {
            Self::Is | Self::Eq => "not ",
            Self::IsNot | Self::NotEq => "",
        }
    }
}

impl std::fmt::Display for EmptyStringCmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let repr = match self {
            Self::Is => "is",
            Self::IsNot => "is not",
            Self::Eq => "==",
            Self::NotEq => "!=",
        };
        write!(f, "{repr}")
    }
}

/// ## What it does
/// Checks for comparisons to empty strings.
///
/// ## Why is this bad?
/// An empty string is falsy, so it is unnecessary to compare it to `""`. If
/// the value can be something else Python considers falsy, such as `None` or
/// `0` or another empty container, then the code is not equivalent.
///
/// ## Example
/// ```python
/// def foo(x):
///     if x == "":
///         print("x is empty")
/// ```
///
/// Use instead:
/// ```python
/// def foo(x):
///     if not x:
///         print("x is empty")
/// ```
///
/// ## References
/// - [Python documentation: Truth Value Testing](https://docs.python.org/3/library/stdtypes.html#truth-value-testing)
#[violation]
pub struct CompareToEmptyString {
    existing: String,
    replacement: String,
}

impl Violation for CompareToEmptyString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`{}` can be simplified to `{}` as an empty string is falsey",
            self.existing, self.replacement,
        )
    }
}

pub(crate) fn compare_to_empty_string(
    checker: &mut Checker,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    // Omit string comparison rules within subscripts. This is mostly commonly used within
    // DataFrame and np.ndarray indexing.
    for parent in checker.semantic().expr_ancestors() {
        if matches!(parent, Expr::Subscript(_)) {
            return;
        }
    }

    let mut first = true;
    for ((lhs, rhs), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows::<(&Expr<_>, &Expr<_>)>()
        .zip(ops)
    {
        if let Ok(op) = EmptyStringCmpOp::try_from(op) {
            if std::mem::take(&mut first) {
                // Check the left-most expression.
                if let Expr::Constant(ast::ExprConstant { value, .. }) = &lhs {
                    if let Constant::Str(s) = value {
                        if s.is_empty() {
                            let constant = checker.generator().constant(value);
                            let expr = checker.generator().expr(rhs);
                            let existing = format!("{constant} {op} {expr}");
                            let replacement = format!("{}{expr}", op.into_unary());
                            checker.diagnostics.push(Diagnostic::new(
                                CompareToEmptyString {
                                    existing,
                                    replacement,
                                },
                                lhs.range(),
                            ));
                        }
                    }
                }
            }

            // Check all right-hand expressions.
            if let Expr::Constant(ast::ExprConstant { value, .. }) = &rhs {
                if let Constant::Str(s) = value {
                    if s.is_empty() {
                        let expr = checker.generator().expr(lhs);
                        let constant = checker.generator().constant(value);
                        let existing = format!("{expr} {op} {constant}");
                        let replacement = format!("{}{expr}", op.into_unary());
                        checker.diagnostics.push(Diagnostic::new(
                            CompareToEmptyString {
                                existing,
                                replacement,
                            },
                            rhs.range(),
                        ));
                    }
                }
            }
        }
    }
}
