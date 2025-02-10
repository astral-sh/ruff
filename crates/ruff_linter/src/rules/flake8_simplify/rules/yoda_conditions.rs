use std::cmp;

use anyhow::Result;
use libcst_native::CompOp;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, CmpOp, Expr, UnaryOp};
use ruff_python_codegen::Stylist;
use ruff_python_stdlib::str::{self};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::helpers::or_space;
use crate::cst::matchers::{match_comparison, transform_expression};
use crate::fix::edits::pad;
use crate::fix::snippet::SourceCodeSnippet;
use crate::Locator;

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
#[derive(ViolationMetadata)]
pub(crate) struct YodaConditions {
    suggestion: Option<SourceCodeSnippet>,
}

impl Violation for YodaConditions {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Yoda condition detected".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let YodaConditions { suggestion } = self;
        suggestion
            .as_ref()
            .and_then(|suggestion| suggestion.full_display())
            .map(|suggestion| format!("Rewrite as `{suggestion}`"))
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

impl From<&Expr> for ConstantLikelihood {
    /// Determine the [`ConstantLikelihood`] of an expression.
    fn from(expr: &Expr) -> Self {
        match expr {
            _ if expr.is_literal_expr() => ConstantLikelihood::Definitely,
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                ConstantLikelihood::from_identifier(attr)
            }
            Expr::Name(ast::ExprName { id, .. }) => ConstantLikelihood::from_identifier(id),
            Expr::Tuple(tuple) => tuple
                .iter()
                .map(ConstantLikelihood::from)
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            Expr::List(list) => list
                .iter()
                .map(ConstantLikelihood::from)
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            Expr::Dict(dict) => dict
                .items
                .iter()
                .flat_map(|item| std::iter::once(&item.value).chain(item.key.as_ref()))
                .map(ConstantLikelihood::from)
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            Expr::BinOp(ast::ExprBinOp { left, right, .. }) => cmp::min(
                ConstantLikelihood::from(&**left),
                ConstantLikelihood::from(&**right),
            ),
            Expr::UnaryOp(ast::ExprUnaryOp {
                op: UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert,
                operand,
                range: _,
            }) => ConstantLikelihood::from(&**operand),
            _ => ConstantLikelihood::Unlikely,
        }
    }
}

impl ConstantLikelihood {
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
    checker: &Checker,
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

    if ConstantLikelihood::from(left) <= ConstantLikelihood::from(right) {
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
        checker.report_diagnostic(diagnostic);
    } else {
        checker.report_diagnostic(Diagnostic::new(
            YodaConditions { suggestion: None },
            expr.range(),
        ));
    }
}
