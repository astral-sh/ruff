use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{
    Boolop, Cmpop, Constant, Expr, ExprContext, ExprKind, Location, Unaryop,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::{contains_effect, create_expr, has_comments, unparse_expr};
use ruff_python_ast::types::Range;
use ruff_python_semantic::context::Context;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for multiple `isinstance` calls on the same target.
///
/// ## Why is this bad?
/// To check if an object is an instance of any one of multiple types
/// or classes, it is unnecessary to use multiple `isinstance` calls, as
/// the second argument of the `isinstance` built-in function accepts a
/// tuple of types and classes.
///
/// Using a single `isinstance` call implements the same behavior with more
/// concise code and clearer intent.
///
/// ## Example
/// ```python
/// if isinstance(obj, int) or isinstance(obj, float):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if isinstance(obj, (int, float)):
///     pass
/// ```
///
/// ## References
/// - [Python: "isinstance"](https://docs.python.org/3/library/functions.html#isinstance)
#[violation]
pub struct DuplicateIsinstanceCall {
    pub name: Option<String>,
    pub fixable: bool,
}

impl Violation for DuplicateIsinstanceCall {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateIsinstanceCall { name, .. } = self;
        if let Some(name) = name {
            format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
        } else {
            format!("Multiple `isinstance` calls for expression, merge into a single call")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|DuplicateIsinstanceCall { name, .. }| {
                if let Some(name) = name {
                    format!("Merge `isinstance` calls for `{name}`")
                } else {
                    format!("Merge `isinstance` calls")
                }
            })
    }
}

#[violation]
pub struct CompareWithTuple {
    pub replacement: String,
}

impl AlwaysAutofixableViolation for CompareWithTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Use `{replacement}` instead of multiple equality comparisons")
    }

    fn autofix_title(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Replace with `{replacement}`")
    }
}

#[violation]
pub struct ExprAndNotExpr {
    pub name: String,
}

impl AlwaysAutofixableViolation for ExprAndNotExpr {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprAndNotExpr { name } = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

#[violation]
pub struct ExprOrNotExpr {
    pub name: String,
}

impl AlwaysAutofixableViolation for ExprOrNotExpr {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprOrNotExpr { name } = self;
        format!("Use `True` instead of `{name} or not {name}`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

#[violation]
pub struct ExprOrTrue;

impl AlwaysAutofixableViolation for ExprOrTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `True` instead of `... or True`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

#[violation]
pub struct ExprAndFalse;

impl AlwaysAutofixableViolation for ExprAndFalse {
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

    // Locate duplicate `isinstance` calls, represented as a map from `ComparableExpr`
    // to indices of the relevant `Expr` instances in `values`.
    let mut duplicates: FxHashMap<ComparableExpr, Vec<usize>> = FxHashMap::default();
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
        if !checker.ctx.is_builtin("isinstance") {
            continue;
        }

        // Collect the target (e.g., `obj` in `isinstance(obj, int)`).
        let target = &args[0];
        duplicates
            .entry(target.into())
            .or_insert_with(Vec::new)
            .push(index);
    }

    // Generate a `Diagnostic` for each duplicate.
    for indices in duplicates.values() {
        if indices.len() > 1 {
            // Grab the target used in each duplicate `isinstance` call (e.g., `obj` in
            // `isinstance(obj, int)`).
            let target = if let ExprKind::Call { args, .. } = &values[indices[0]].node {
                args.get(0).expect("`isinstance` should have two arguments")
            } else {
                unreachable!("Indices should only contain `isinstance` calls")
            };
            let fixable = !contains_effect(target, |id| checker.ctx.is_builtin(id));
            let mut diagnostic = Diagnostic::new(
                DuplicateIsinstanceCall {
                    name: if let ExprKind::Name { id, .. } = &target.node {
                        Some(id.to_string())
                    } else {
                        None
                    },
                    fixable,
                },
                Range::from(expr),
            );
            if fixable && checker.patch(diagnostic.kind.rule()) {
                // Grab the types used in each duplicate `isinstance` call (e.g., `int` and `str`
                // in `isinstance(obj, int) or isinstance(obj, str)`).
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
                        target.clone(),
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
                diagnostic.set_fix(Edit::replacement(
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
            .any(|expr| contains_effect(expr, |id| checker.ctx.is_builtin(id)))
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
            Range::from(expr),
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
            diagnostic.set_fix(Edit::replacement(
                unparse_expr(&in_expr, checker.stylist),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// SIM220
pub fn expr_and_not_expr(checker: &mut Checker, expr: &Expr) {
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

    if contains_effect(expr, |id| checker.ctx.is_builtin(id)) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    ExprAndNotExpr {
                        name: id.to_string(),
                    },
                    Range::from(expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Edit::replacement(
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
pub fn expr_or_not_expr(checker: &mut Checker, expr: &Expr) {
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

    if contains_effect(expr, |id| checker.ctx.is_builtin(id)) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    ExprOrNotExpr {
                        name: id.to_string(),
                    },
                    Range::from(expr),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Edit::replacement(
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

pub fn is_short_circuit(
    ctx: &Context,
    expr: &Expr,
    expected_op: &Boolop,
) -> Option<(Location, Location)> {
    let ExprKind::BoolOp { op, values, } = &expr.node else {
        return None;
    };
    if op != expected_op {
        return None;
    }
    let short_circuit_value = match op {
        Boolop::And => false,
        Boolop::Or => true,
    };

    let mut location = expr.location;
    for (value, next_value) in values.iter().tuple_windows() {
        // Keep track of the location of the furthest-right, non-effectful expression.
        if contains_effect(value, |id| ctx.is_builtin(id)) {
            location = next_value.location;
            continue;
        }

        // If the current expression is a constant, and it matches the short-circuit value, then
        // we can return the location of the expression. This should only trigger if the
        // short-circuit expression is the first expression in the list; otherwise, we'll see it
        // as `next_value` before we see it as `value`.
        if let ExprKind::Constant {
            value: Constant::Bool(bool),
            ..
        } = &value.node
        {
            if bool == &short_circuit_value {
                return Some((location, expr.end_location.unwrap()));
            }
        }

        // If the next expression is a constant, and it matches the short-circuit value, then
        // we can return the location of the expression.
        if let ExprKind::Constant {
            value: Constant::Bool(bool),
            ..
        } = &next_value.node
        {
            if bool == &short_circuit_value {
                return Some((location, expr.end_location.unwrap()));
            }
        }
    }
    None
}

/// SIM222
pub fn expr_or_true(checker: &mut Checker, expr: &Expr) {
    let Some((location, end_location)) = is_short_circuit(&checker.ctx, expr, &Boolop::Or) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        ExprOrTrue,
        Range {
            location,
            end_location,
        },
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            "True".to_string(),
            location,
            end_location,
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM223
pub fn expr_and_false(checker: &mut Checker, expr: &Expr) {
    let Some((location, end_location)) = is_short_circuit(&checker.ctx, expr, &Boolop::And) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        ExprAndFalse,
        Range {
            location,
            end_location,
        },
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            "False".to_string(),
            location,
            end_location,
        ));
    }
    checker.diagnostics.push(diagnostic);
}
