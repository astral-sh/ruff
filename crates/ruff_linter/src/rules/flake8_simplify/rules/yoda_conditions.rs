use std::cmp;

use anyhow::Result;
use libcst_native::CompOp;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, CmpOp, Expr, UnaryOp};
use ruff_python_codegen::Stylist;
use ruff_python_stdlib::str::{self};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::helpers::or_space;
use crate::cst::matchers::{match_comparison, transform_expression};
use crate::fix::edits::pad;
use crate::fix::snippet::SourceCodeSnippet;
use crate::settings::types::PreviewMode;

/// ## What it does
/// Checks for conditions that position a constant on the left-hand side of the
/// comparison operator, rather than the right-hand side.
///
/// ## Why is this bad?
/// These conditions (sometimes referred to as "Yoda conditions") are less
/// readable than conditions that place the variable on the left-hand side of
/// the comparison operator.
///
/// In some languages, Yoda conditions are used to prevent accidental
/// assignment in conditions (i.e., accidental uses of the `=` operator,
/// instead of the `==` operator). However, Python does not allow assignments
/// in conditions unless using the `:=` operator, so Yoda conditions provide
/// no benefit in this regard.
///
/// ## Example
/// ```python
/// if "Foo" == foo:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if foo == "Foo":
///     ...
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
/// - [Python documentation: Assignment statements](https://docs.python.org/3/reference/simple_stmts.html#assignment-statements)
#[violation]
pub struct YodaConditions {
    suggestion: Option<SourceCodeSnippet>,
}

impl Violation for YodaConditions {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let YodaConditions { suggestion } = self;
        if let Some(suggestion) = suggestion
            .as_ref()
            .and_then(SourceCodeSnippet::full_display)
        {
            format!("Yoda conditions are discouraged, use `{suggestion}` instead")
        } else {
            format!("Yoda conditions are discouraged")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let YodaConditions { suggestion } = self;
        suggestion.as_ref().map(|suggestion| {
            if let Some(suggestion) = suggestion.full_display() {
                format!("Replace Yoda condition with `{suggestion}`")
            } else {
                format!("Replace Yoda condition")
            }
        })
    }
}

/// Comparisons left-hand side must not be more [`ConstantLikelihood`] than the right-hand side.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
enum ConstantLikelihood {
    /// The expression is unlikely to be a constant (e.g., `foo` or `foo(bar)`).
    Unlikely = 0,

    /// The expression is likely to be a constant (e.g., `FOO`).
    Probably = 1,

    /// The expression is definitely a constant (e.g., `42` or `"foo"`).
    Definitely = 2,
}

impl ConstantLikelihood {
    /// Determine the [`ConstantLikelihood`] of an expression.
    fn from_expression(expr: &Expr, preview: PreviewMode) -> Self {
        match expr {
            _ if expr.is_literal_expr() => ConstantLikelihood::Definitely,
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                ConstantLikelihood::from_identifier(attr)
            }
            Expr::Name(ast::ExprName { id, .. }) => ConstantLikelihood::from_identifier(id),
            Expr::Tuple(ast::ExprTuple { elts, .. }) => elts
                .iter()
                .map(|expr| ConstantLikelihood::from_expression(expr, preview))
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            Expr::List(ast::ExprList { elts, .. }) if preview.is_enabled() => elts
                .iter()
                .map(|expr| ConstantLikelihood::from_expression(expr, preview))
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            Expr::Dict(ast::ExprDict { values: vs, .. }) if preview.is_enabled() => {
                if vs.is_empty() {
                    ConstantLikelihood::Definitely
                } else {
                    ConstantLikelihood::Probably
                }
            }
            Expr::BinOp(ast::ExprBinOp { left, right, .. }) => cmp::min(
                ConstantLikelihood::from_expression(left, preview),
                ConstantLikelihood::from_expression(right, preview),
            ),
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert,
                operand,
                range: _,
            }) => ConstantLikelihood::from_expression(operand, preview),
            _ => ConstantLikelihood::Unlikely,
        }
    }

    /// Determine the [`ConstantLikelihood`] of an identifier.
    fn from_identifier(identifier: &str) -> Self {
        if str::is_cased_uppercase(identifier) {
            ConstantLikelihood::Probably
        } else {
            ConstantLikelihood::Unlikely
        }
    }
}

/// Generate a fix to reverse a comparison.
fn reverse_comparison(expr: &Expr, locator: &Locator, stylist: &Stylist) -> Result<String> {
    let range = expr.range();
    let source_code = locator.slice(range);

    transform_expression(source_code, stylist, |mut expression| {
        let comparison = match_comparison(&mut expression)?;

        let left = (*comparison.left).clone();

        // Copy the right side to the left side.
        comparison.left = Box::new(comparison.comparisons[0].comparator.clone());

        // Copy the left side to the right side.
        comparison.comparisons[0].comparator = left;

        // Reverse the operator.
        let op = comparison.comparisons[0].operator.clone();
        comparison.comparisons[0].operator = match op {
            CompOp::LessThan {
                whitespace_before,
                whitespace_after,
            } => CompOp::GreaterThan {
                whitespace_before: or_space(whitespace_before),
                whitespace_after: or_space(whitespace_after),
            },
            CompOp::GreaterThan {
                whitespace_before,
                whitespace_after,
            } => CompOp::LessThan {
                whitespace_before: or_space(whitespace_before),
                whitespace_after: or_space(whitespace_after),
            },
            CompOp::LessThanEqual {
                whitespace_before,
                whitespace_after,
            } => CompOp::GreaterThanEqual {
                whitespace_before: or_space(whitespace_before),
                whitespace_after: or_space(whitespace_after),
            },
            CompOp::GreaterThanEqual {
                whitespace_before,
                whitespace_after,
            } => CompOp::LessThanEqual {
                whitespace_before: or_space(whitespace_before),
                whitespace_after: or_space(whitespace_after),
            },
            CompOp::Equal {
                whitespace_before,
                whitespace_after,
            } => CompOp::Equal {
                whitespace_before: or_space(whitespace_before),
                whitespace_after: or_space(whitespace_after),
            },
            CompOp::NotEqual {
                whitespace_before,
                whitespace_after,
            } => CompOp::NotEqual {
                whitespace_before: or_space(whitespace_before),
                whitespace_after: or_space(whitespace_after),
            },
            _ => panic!("Expected comparison operator"),
        };

        Ok(expression)
    })
}

/// SIM300
pub(crate) fn yoda_conditions(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    if !matches!(
        op,
        CmpOp::Eq | CmpOp::NotEq | CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE,
    ) {
        return;
    }

    if ConstantLikelihood::from_expression(left, checker.settings.preview)
        <= ConstantLikelihood::from_expression(right, checker.settings.preview)
    {
        return;
    }

    if let Ok(suggestion) = reverse_comparison(expr, checker.locator(), checker.stylist()) {
        let mut diagnostic = Diagnostic::new(
            YodaConditions {
                suggestion: Some(SourceCodeSnippet::new(suggestion.clone())),
            },
            expr.range(),
        );
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(suggestion, expr.range(), checker.locator()),
            expr.range(),
        )));
        checker.diagnostics.push(diagnostic);
    } else {
        checker.diagnostics.push(Diagnostic::new(
            YodaConditions { suggestion: None },
            expr.range(),
        ));
    }
}
