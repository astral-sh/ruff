use anyhow::Result;
use libcst_native::{Codegen, CodegenState, CompOp};
use ruff_macros::{define_violation, derive_message_formats};
use ruff_python::string::{self};
use rustpython_parser::ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_comparison, match_expression};
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::{Locator, Stylist};
use crate::violation::{Availability, Violation};
use crate::AutofixKind;

define_violation!(
    pub struct YodaConditions {
        pub suggestion: Option<String>,
    }
);
impl Violation for YodaConditions {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let YodaConditions { suggestion } = self;
        if let Some(suggestion) = suggestion {
            format!("Yoda conditions are discouraged, use `{suggestion}` instead")
        } else {
            format!("Yoda conditions are discouraged")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let YodaConditions { suggestion, .. } = self;
        if suggestion.is_some() {
            Some(|YodaConditions { suggestion }| {
                let suggestion = suggestion.as_ref().unwrap();
                format!("Replace Yoda condition with `{suggestion}`")
            })
        } else {
            None
        }
    }
}

/// Return `true` if an [`Expr`] is a constant or a constant-like name.
fn is_constant_like(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => string::is_upper(attr),
        ExprKind::Constant { .. } => true,
        ExprKind::Tuple { elts, .. } => elts.iter().all(is_constant_like),
        ExprKind::Name { id, .. } => string::is_upper(id),
        _ => false,
    }
}

/// Generate a fix to reverse a comparison.
fn reverse_comparison(expr: &Expr, locator: &Locator, stylist: &Stylist) -> Result<String> {
    let range = Range::from_located(expr);
    let contents = locator.slice_source_code_range(&range);

    let mut expression = match_expression(contents)?;
    let mut comparison = match_comparison(&mut expression)?;

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
        _ => unreachable!("Expected comparison operator"),
    };

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    expression.codegen(&mut state);
    Ok(state.to_string())
}

/// SIM300
pub fn yoda_conditions(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    if !matches!(
        op,
        Cmpop::Eq | Cmpop::NotEq | Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE,
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
            Range::from_located(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                suggestion,
                left.location,
                right.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    } else {
        checker.diagnostics.push(Diagnostic::new(
            YodaConditions { suggestion: None },
            Range::from_located(expr),
        ));
    }
}
