use std::cmp;

use anyhow::Result;
use libcst_native::CompOp;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{CmpOp, Expr, ExprCompare, UnaryOp, flat};
use ruff_python_codegen::Stylist;
use ruff_python_stdlib::str::{self};
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::cst::helpers::or_space;
use crate::cst::matchers::{match_comparison, transform_expression};
use crate::fix::edits::pad;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{Edit, Fix, FixAvailability, Violation};

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
#[violation_metadata(stable_since = "v0.0.207", category = Category::Pedantic)]
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

impl From<flat::View<'_, Expr>> for ConstantLikelihood {
    /// Determine the [`ConstantLikelihood`] of an expression.
    fn from(expr: flat::View<'_, Expr>) -> Self {
        match expr.classify() {
            _ if expr.is_literal_expr() => ConstantLikelihood::Definitely,
            flat::ExprKind::Attribute(attribute) => {
                ConstantLikelihood::from_identifier(attribute.attr().as_str())
            }
            flat::ExprKind::Name(name) => ConstantLikelihood::from_identifier(name.id()),
            flat::ExprKind::Tuple(tuple) => tuple
                .iter()
                .map(ConstantLikelihood::from)
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            flat::ExprKind::List(list) => list
                .elts()
                .iter()
                .map(ConstantLikelihood::from)
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            flat::ExprKind::Dict(dict) => dict
                .items()
                .iter()
                .flat_map(|item| std::iter::once(item.value).chain(item.key))
                .map(ConstantLikelihood::from)
                .min()
                .unwrap_or(ConstantLikelihood::Definitely),
            flat::ExprKind::BinOp(binary) => cmp::min(
                ConstantLikelihood::from(binary.left()),
                ConstantLikelihood::from(binary.right()),
            ),
            flat::ExprKind::UnaryOp(unary)
                if matches!(unary.op(), UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert) =>
            {
                ConstantLikelihood::from(unary.operand())
            }
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
fn reverse_comparison(range: TextRange, locator: &Locator, stylist: &Stylist) -> Result<String> {
    let source_code = locator.slice(range);

    transform_expression(source_code, stylist, |mut expression| {
        let comparison = match_comparison(&mut expression)?;

        let left = (*comparison.left).clone();

        // Copy the right side to the left side.
        *comparison.left = comparison.comparisons[0].comparator.clone();

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
pub(crate) fn yoda_conditions(checker: &Checker, compare: flat::View<'_, ExprCompare>) {
    let Some((left, op, right)) = compare.as_single() else {
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

    if let Ok(suggestion) =
        reverse_comparison(compare.range(), checker.locator(), checker.stylist())
    {
        let mut diagnostic = checker.report_diagnostic(
            YodaConditions {
                suggestion: Some(SourceCodeSnippet::new(suggestion.clone())),
            },
            compare.range(),
        );
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(suggestion, compare.range(), checker.locator()),
            compare.range(),
        )));
    } else {
        checker.report_diagnostic(YodaConditions { suggestion: None }, compare.range());
    }
}
