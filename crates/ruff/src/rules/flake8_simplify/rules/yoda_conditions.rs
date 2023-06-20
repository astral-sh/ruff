use anyhow::Result;
use libcst_native::CompOp;
use rustpython_parser::ast::{self, CmpOp, Expr, Ranged, UnaryOp};

use crate::autofix::codemods::CodegenStylist;
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_stdlib::str::{self};

use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_comparison, match_expression};
use crate::registry::AsRule;

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
    pub suggestion: Option<String>,
}

impl Violation for YodaConditions {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let YodaConditions { suggestion } = self;
        if let Some(suggestion) = suggestion {
            format!("Yoda conditions are discouraged, use `{suggestion}` instead")
        } else {
            format!("Yoda conditions are discouraged")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let YodaConditions { suggestion } = self;
        suggestion
            .as_ref()
            .map(|suggestion| format!("Replace Yoda condition with `{suggestion}`"))
    }
}

/// Return `true` if an [`Expr`] is a constant or a constant-like name.
fn is_constant_like(expr: &Expr) -> bool {
    match expr {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => str::is_cased_uppercase(attr),
        Expr::Constant(_) => true,
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().all(is_constant_like),
        Expr::Name(ast::ExprName { id, .. }) => str::is_cased_uppercase(id),
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert,
            operand,
            range: _,
        }) => operand.is_constant_expr(),
        _ => false,
    }
}

/// Generate a fix to reverse a comparison.
fn reverse_comparison(expr: &Expr, locator: &Locator, stylist: &Stylist) -> Result<String> {
    let range = expr.range();
    let contents = locator.slice(range);

    let mut expression = match_expression(contents)?;
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
            whitespace_before,
            whitespace_after,
        },
        CompOp::GreaterThan {
            whitespace_before,
            whitespace_after,
        } => CompOp::LessThan {
            whitespace_before,
            whitespace_after,
        },
        CompOp::LessThanEqual {
            whitespace_before,
            whitespace_after,
        } => CompOp::GreaterThanEqual {
            whitespace_before,
            whitespace_after,
        },
        CompOp::GreaterThanEqual {
            whitespace_before,
            whitespace_after,
        } => CompOp::LessThanEqual {
            whitespace_before,
            whitespace_after,
        },
        CompOp::Equal {
            whitespace_before,
            whitespace_after,
        } => CompOp::Equal {
            whitespace_before,
            whitespace_after,
        },
        CompOp::NotEqual {
            whitespace_before,
            whitespace_after,
        } => CompOp::NotEqual {
            whitespace_before,
            whitespace_after,
        },
        _ => panic!("Expected comparison operator"),
    };

    Ok(expression.codegen_stylist(stylist))
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

    if !is_constant_like(left) || is_constant_like(right) {
        return;
    }

    if let Ok(suggestion) = reverse_comparison(expr, checker.locator, checker.stylist) {
        let mut diagnostic = Diagnostic::new(
            YodaConditions {
                suggestion: Some(suggestion.to_string()),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                suggestion,
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    } else {
        checker.diagnostics.push(Diagnostic::new(
            YodaConditions { suggestion: None },
            expr.range(),
        ));
    }
}
