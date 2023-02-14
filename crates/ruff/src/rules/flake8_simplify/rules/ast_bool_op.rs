use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Boolop, Cmpop, Constant, Expr, ExprContext, ExprKind, Unaryop};

use crate::ast::helpers::{contains_effect, create_expr, has_comments, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct DuplicateIsinstanceCall {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for DuplicateIsinstanceCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateIsinstanceCall { name } = self;
        format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
    }

    fn autofix_title(&self) -> String {
        let DuplicateIsinstanceCall { name } = self;
        format!("Merge `isinstance` calls for `{name}`")
    }
}

define_violation!(
    pub struct CompareWithTuple {
        pub replacement: String,
    }
);
impl AlwaysAutofixableViolation for CompareWithTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Use `{replacement}` instead of multiple equality comparisons")
    }

    fn autofix_title(&self) -> String {
        let CompareWithTuple { replacement, .. } = self;
        format!("Replace with `{replacement}`")
    }
}

define_violation!(
    pub struct AAndNotA {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for AAndNotA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AAndNotA { name } = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

define_violation!(
    pub struct AOrNotA {
        pub name: String,
    }
);
impl AlwaysAutofixableViolation for AOrNotA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AOrNotA { name } = self;
        format!("Use `True` instead of `{name} or not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

define_violation!(
    pub struct OrTrue;
);
impl AlwaysAutofixableViolation for OrTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `True` instead of `... or True`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

define_violation!(
    pub struct AndFalse;
);
impl AlwaysAutofixableViolation for AndFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `False` instead of `... and False`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

/// Return `true` if two `Expr` instances are equivalent names.
fn is_same_expr<'a>(a: &'a Expr, b: &'a Expr) -> Option<&'a str> {
    if let (ExprKind::Name { id: a, .. }, ExprKind::Name { id: b, .. }) = (&a.node, &b.node) {
        if a == b {
            return Some(a);
        }
    }
    None
}

/// SIM101
pub fn duplicate_isinstance_call(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values } = &expr.node else {
        return;
    };

    // Locate duplicate `isinstance` calls, represented as a map from argument name
    // to indices of the relevant `Expr` instances in `values`.
    let mut duplicates = BTreeMap::new();
    for (index, call) in values.iter().enumerate() {
        // Verify that this is an `isinstance` call.
        let ExprKind::Call { func, args, keywords } = &call.node else {
            continue;
        };
        if args.len() != 2 {
            continue;
        }
        if !keywords.is_empty() {
            continue;
        }
        let ExprKind::Name { id: func_name, .. } = &func.node else {
            continue;
        };
        if func_name != "isinstance" {
            continue;
        }
        if !checker.is_builtin("isinstance") {
            continue;
        }

        // Collect the name of the argument.
        let ExprKind::Name { id: arg_name, .. } = &args[0].node else {
            continue;
        };
        duplicates
            .entry(arg_name.as_str())
            .or_insert_with(Vec::new)
            .push(index);
    }

    // Generate a `Diagnostic` for each duplicate.
    for (arg_name, indices) in duplicates {
        if indices.len() > 1 {
            let mut diagnostic = Diagnostic::new(
                DuplicateIsinstanceCall {
                    name: arg_name.to_string(),
                },
                Range::from_located(expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Grab the types used in each duplicate `isinstance` call.
                let types: Vec<&Expr> = indices
                    .iter()
                    .map(|index| &values[*index])
                    .map(|expr| {
                        let ExprKind::Call { args, ..} = &expr.node else {
                            unreachable!("Indices should only contain `isinstance` calls")
                        };
                        args.get(1).expect("`isinstance` should have two arguments")
                    })
                    .collect();

                // Generate a single `isinstance` call.
                let call = create_expr(ExprKind::Call {
                    func: Box::new(create_expr(ExprKind::Name {
                        id: "isinstance".to_string(),
                        ctx: ExprContext::Load,
                    })),
                    args: vec![
                        create_expr(ExprKind::Name {
                            id: arg_name.to_string(),
                            ctx: ExprContext::Load,
                        }),
                        create_expr(ExprKind::Tuple {
                            // Flatten all the types used across the `isinstance` calls.
                            elts: types
                                .iter()
                                .flat_map(|value| {
                                    if let ExprKind::Tuple { elts, .. } = &value.node {
                                        Left(elts.iter())
                                    } else {
                                        Right(iter::once(*value))
                                    }
                                })
                                .map(Clone::clone)
                                .collect(),
                            ctx: ExprContext::Load,
                        }),
                    ],
                    keywords: vec![],
                });

                // Generate the combined `BoolOp`.
                let bool_op = create_expr(ExprKind::BoolOp {
                    op: Boolop::Or,
                    values: iter::once(call)
                        .chain(
                            values
                                .iter()
                                .enumerate()
                                .filter(|(index, _)| !indices.contains(index))
                                .map(|(_, elt)| elt.clone()),
                        )
                        .collect(),
                });

                // Populate the `Fix`. Replace the _entire_ `BoolOp`. Note that if we have
                // multiple duplicates, the fixes will conflict.
                diagnostic.amend(Fix::replacement(
                    unparse_expr(&bool_op, checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn match_eq_target(expr: &Expr) -> Option<(&str, &Expr)> {
    let ExprKind::Compare { left, ops, comparators } = &expr.node else {
        return None;
    };
    if ops.len() != 1 || comparators.len() != 1 {
        return None;
    }
    if !matches!(&ops[0], Cmpop::Eq) {
        return None;
    }
    let ExprKind::Name { id, .. } = &left.node else {
        return None;
    };
    let comparator = &comparators[0];
    if !matches!(&comparator.node, ExprKind::Name { .. }) {
        return None;
    }
    Some((id, comparator))
}

/// SIM109
pub fn compare_with_tuple(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values } = &expr.node else {
        return;
    };

    // Given `a == "foo" or a == "bar"`, we generate `{"a": [(0, "foo"), (1,
    // "bar")]}`.
    let mut id_to_comparators: BTreeMap<&str, Vec<(usize, &Expr)>> = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        if let Some((id, comparator)) = match_eq_target(value) {
            id_to_comparators
                .entry(id)
                .or_default()
                .push((index, comparator));
        }
    }

    for (id, matches) in id_to_comparators {
        if matches.len() == 1 {
            continue;
        }

        let (indices, comparators): (Vec<_>, Vec<_>) = matches.iter().copied().unzip();

        // Avoid rewriting (e.g.) `a == "foo" or a == f()`.
        if comparators
            .iter()
            .any(|expr| contains_effect(checker, expr))
        {
            continue;
        }

        // Avoid removing comments.
        if has_comments(expr, checker.locator) {
            continue;
        }

        // Create a `x in (a, b)` expression.
        let in_expr = create_expr(ExprKind::Compare {
            left: Box::new(create_expr(ExprKind::Name {
                id: id.to_string(),
                ctx: ExprContext::Load,
            })),
            ops: vec![Cmpop::In],
            comparators: vec![create_expr(ExprKind::Tuple {
                elts: comparators.into_iter().map(Clone::clone).collect(),
                ctx: ExprContext::Load,
            })],
        });
        let mut diagnostic = Diagnostic::new(
            CompareWithTuple {
                replacement: unparse_expr(&in_expr, checker.stylist),
            },
            Range::from_located(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            let unmatched: Vec<Expr> = values
                .iter()
                .enumerate()
                .filter(|(index, _)| !indices.contains(index))
                .map(|(_, elt)| elt.clone())
                .collect();
            let in_expr = if unmatched.is_empty() {
                in_expr
            } else {
                // Wrap in a `x in (a, b) or ...` boolean operation.
                create_expr(ExprKind::BoolOp {
                    op: Boolop::Or,
                    values: iter::once(in_expr).chain(unmatched).collect(),
                })
            };
            diagnostic.amend(Fix::replacement(
                unparse_expr(&in_expr, checker.stylist),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// SIM220
pub fn a_and_not_a(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::And, values, } = &expr.node else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } = &expr.node
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    if contains_effect(checker, expr) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    AAndNotA {
                        name: id.to_string(),
                    },
                    Range::from_located(expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.amend(Fix::replacement(
                        "False".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// SIM221
pub fn a_or_not_a(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values, } = &expr.node else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } = &expr.node
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    if contains_effect(checker, expr) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    AOrNotA {
                        name: id.to_string(),
                    },
                    Range::from_located(expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.amend(Fix::replacement(
                        "True".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// SIM222
pub fn or_true(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values, } = &expr.node else {
        return;
    };
    if contains_effect(checker, expr) {
        return;
    }
    for value in values {
        if let ExprKind::Constant {
            value: Constant::Bool(true),
            ..
        } = &value.node
        {
            let mut diagnostic = Diagnostic::new(OrTrue, Range::from_located(value));
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::replacement(
                    "True".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// SIM223
pub fn and_false(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::And, values, } = &expr.node else {
        return;
    };
    if contains_effect(checker, expr) {
        return;
    }
    for value in values {
        if let ExprKind::Constant {
            value: Constant::Bool(false),
            ..
        } = &value.node
        {
            let mut diagnostic = Diagnostic::new(AndFalse, Range::from_located(value));
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::replacement(
                    "False".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
