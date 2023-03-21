use anyhow::bail;
use itertools::Itertools;
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{unparse_constant, unparse_expr};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum EmptyStringCmpop {
    Is,
    IsNot,
    Eq,
    NotEq,
}

impl TryFrom<&Cmpop> for EmptyStringCmpop {
    type Error = anyhow::Error;

    fn try_from(value: &Cmpop) -> Result<Self, Self::Error> {
        match value {
            Cmpop::Is => Ok(Self::Is),
            Cmpop::IsNot => Ok(Self::IsNot),
            Cmpop::Eq => Ok(Self::Eq),
            Cmpop::NotEq => Ok(Self::NotEq),
            _ => bail!("{value:?} cannot be converted to EmptyStringCmpop"),
        }
    }
}

impl EmptyStringCmpop {
    pub fn into_unary(self) -> &'static str {
        match self {
            Self::Is | Self::Eq => "not ",
            Self::IsNot | Self::NotEq => "",
        }
    }
}

impl std::fmt::Display for EmptyStringCmpop {
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

#[violation]
pub struct CompareToEmptyString {
    pub existing: String,
    pub replacement: String,
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

pub fn compare_to_empty_string(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    // Omit string comparison rules within subscripts. This is mostly commonly used within
    // DataFrame and np.ndarray indexing.
    for parent in checker.ctx.expr_ancestors() {
        if matches!(parent.node, ExprKind::Subscript { .. }) {
            return;
        }
    }

    let mut first = true;
    for ((lhs, rhs), op) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows::<(&Expr<_>, &Expr<_>)>()
        .zip(ops)
    {
        if let Ok(op) = EmptyStringCmpop::try_from(op) {
            if std::mem::take(&mut first) {
                // Check the left-most expression.
                if let ExprKind::Constant { value, .. } = &lhs.node {
                    if let Constant::Str(s) = value {
                        if s.is_empty() {
                            let constant = unparse_constant(value, checker.stylist);
                            let expr = unparse_expr(rhs, checker.stylist);
                            let existing = format!("{constant} {op} {expr}");
                            let replacement = format!("{}{expr}", op.into_unary());
                            checker.diagnostics.push(Diagnostic::new(
                                CompareToEmptyString {
                                    existing,
                                    replacement,
                                },
                                Range::from(lhs),
                            ));
                        }
                    }
                }
            }

            // Check all right-hand expressions.
            if let ExprKind::Constant { value, .. } = &rhs.node {
                if let Constant::Str(s) = value {
                    if s.is_empty() {
                        let expr = unparse_expr(lhs, checker.stylist);
                        let constant = unparse_constant(value, checker.stylist);
                        let existing = format!("{expr} {op} {constant}");
                        let replacement = format!("{}{expr}", op.into_unary());
                        checker.diagnostics.push(Diagnostic::new(
                            CompareToEmptyString {
                                existing,
                                replacement,
                            },
                            Range::from(rhs),
                        ));
                    }
                }
            }
        }
    }
}
