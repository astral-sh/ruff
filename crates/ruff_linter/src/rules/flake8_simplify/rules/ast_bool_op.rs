use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};
use itertools::Itertools;
use ruff_python_ast::{self as ast, Arguments, BoolOp, CmpOp, Expr, ExprContext, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::{contains_effect, Truthiness};
use ruff_python_ast::name::Name;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_codegen::Generator;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;

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
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
#[violation]
pub struct DuplicateIsinstanceCall {
    name: Option<String>,
}

impl Violation for DuplicateIsinstanceCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateIsinstanceCall { name } = self;
        if let Some(name) = name {
            format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
        } else {
            format!("Multiple `isinstance` calls for expression, merge into a single call")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let DuplicateIsinstanceCall { name } = self;

        Some(if let Some(name) = name {
            format!("Merge `isinstance` calls for `{name}`")
        } else {
            "Merge `isinstance` calls".to_string()
        })
    }
}

/// ## What it does
/// Checks for boolean expressions that contain multiple equality comparisons
/// to the same value.
///
/// ## Why is this bad?
/// To check if an object is equal to any one of multiple values, it's more
/// concise to use the `in` operator with a tuple of values.
///
/// ## Example
/// ```python
/// if foo == x or foo == y:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if foo in (x, y):
///     ...
/// ```
///
/// ## References
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
#[violation]
pub struct CompareWithTuple {
    replacement: String,
}

impl AlwaysFixableViolation for CompareWithTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Use `{replacement}` instead of multiple equality comparisons")
    }

    fn fix_title(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Replace with `{replacement}`")
    }
}

/// ## What it does
/// Checks for `and` expressions that include both an expression and its
/// negation.
///
/// ## Why is this bad?
/// An `and` expression that includes both an expression and its negation will
/// always evaluate to `False`.
///
/// ## Example
/// ```python
/// x and not x
/// ```
///
/// ## References
/// - [Python documentation: Boolean operations](https://docs.python.org/3/reference/expressions.html#boolean-operations)
#[violation]
pub struct ExprAndNotExpr {
    name: String,
}

impl AlwaysFixableViolation for ExprAndNotExpr {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprAndNotExpr { name } = self;
        format!("Use `False` instead of `{name} and not {name}`")
    }

    fn fix_title(&self) -> String {
        "Replace with `False`".to_string()
    }
}

/// ## What it does
/// Checks for `or` expressions that include both an expression and its
/// negation.
///
/// ## Why is this bad?
/// An `or` expression that includes both an expression and its negation will
/// always evaluate to `True`.
///
/// ## Example
/// ```python
/// x or not x
/// ```
///
/// ## References
/// - [Python documentation: Boolean operations](https://docs.python.org/3/reference/expressions.html#boolean-operations)
#[violation]
pub struct ExprOrNotExpr {
    name: String,
}

impl AlwaysFixableViolation for ExprOrNotExpr {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprOrNotExpr { name } = self;
        format!("Use `True` instead of `{name} or not {name}`")
    }

    fn fix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ContentAround {
    Before,
    After,
    Both,
}

/// ## What it does
/// Checks for `or` expressions that contain truthy values.
///
/// ## Why is this bad?
/// If the expression is used as a condition, it can be replaced in-full with
/// `True`.
///
/// In other cases, the expression can be short-circuited to the first truthy
/// value.
///
/// By using `True` (or the first truthy value), the code is more concise
/// and easier to understand, since it no longer contains redundant conditions.
///
/// ## Example
/// ```python
/// if x or [1] or y:
///     pass
///
/// a = x or [1] or y
/// ```
///
/// Use instead:
/// ```python
/// if True:
///     pass
///
/// a = x or [1]
/// ```
#[violation]
pub struct ExprOrTrue {
    expr: String,
    remove: ContentAround,
}

impl AlwaysFixableViolation for ExprOrTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprOrTrue { expr, remove } = self;
        let replaced = match remove {
            ContentAround::After => format!("{expr} or ..."),
            ContentAround::Before => format!("... or {expr}"),
            ContentAround::Both => format!("... or {expr} or ..."),
        };
        format!("Use `{expr}` instead of `{replaced}`")
    }

    fn fix_title(&self) -> String {
        let ExprOrTrue { expr, .. } = self;
        format!("Replace with `{expr}`")
    }
}

/// ## What it does
/// Checks for `and` expressions that contain falsey values.
///
/// ## Why is this bad?
/// If the expression is used as a condition, it can be replaced in-full with
/// `False`.
///
/// In other cases, the expression can be short-circuited to the first falsey
/// value.
///
/// By using `False` (or the first falsey value), the code is more concise
/// and easier to understand, since it no longer contains redundant conditions.
///
/// ## Example
/// ```python
/// if x and [] and y:
///     pass
///
/// a = x and [] and y
/// ```
///
/// Use instead:
/// ```python
/// if False:
///     pass
///
/// a = x and []
/// ```
#[violation]
pub struct ExprAndFalse {
    expr: String,
    remove: ContentAround,
}

impl AlwaysFixableViolation for ExprAndFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExprAndFalse { expr, remove } = self;
        let replaced = match remove {
            ContentAround::After => format!(r#"{expr} and ..."#),
            ContentAround::Before => format!("... and {expr}"),
            ContentAround::Both => format!("... and {expr} and ..."),
        };
        format!("Use `{expr}` instead of `{replaced}`")
    }

    fn fix_title(&self) -> String {
        let ExprAndFalse { expr, .. } = self;
        format!("Replace with `{expr}`")
    }
}

/// Return `true` if two `Expr` instances are equivalent names.
pub(crate) fn is_same_expr<'a>(a: &'a Expr, b: &'a Expr) -> Option<&'a str> {
    if let (Expr::Name(ast::ExprName { id: a, .. }), Expr::Name(ast::ExprName { id: b, .. })) =
        (&a, &b)
    {
        if a == b {
            return Some(a);
        }
    }
    None
}

/// If `call` is an `isinstance()` call, return its target.
fn isinstance_target<'a>(call: &'a Expr, semantic: &'a SemanticModel) -> Option<&'a Expr> {
    // Verify that this is an `isinstance` call.
    let ast::ExprCall {
        func,
        arguments:
            Arguments {
                args,
                keywords,
                range: _,
            },
        range: _,
    } = call.as_call_expr()?;
    if args.len() != 2 {
        return None;
    }
    if !keywords.is_empty() {
        return None;
    }
    if !semantic.match_builtin_expr(func, "isinstance") {
        return None;
    }

    // Collect the target (e.g., `obj` in `isinstance(obj, int)`).
    Some(&args[0])
}

/// SIM101
pub(crate) fn duplicate_isinstance_call(checker: &mut Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        range: _,
    }) = expr
    else {
        return;
    };

    // Locate duplicate `isinstance` calls, represented as a vector of vectors
    // of indices of the relevant `Expr` instances in `values`.
    let mut duplicates: Vec<Vec<usize>> = Vec::new();
    let mut last_target_option: Option<ComparableExpr> = None;
    for (index, call) in values.iter().enumerate() {
        let Some(target) = isinstance_target(call, checker.semantic()) else {
            last_target_option = None;
            continue;
        };

        if last_target_option
            .as_ref()
            .is_some_and(|last_target| *last_target == ComparableExpr::from(target))
        {
            duplicates
                .last_mut()
                .expect("last_target should have a corresponding entry")
                .push(index);
        } else {
            last_target_option = Some(target.into());
            duplicates.push(vec![index]);
        }
    }

    // Generate a `Diagnostic` for each duplicate.
    for indices in duplicates {
        if indices.len() > 1 {
            // Grab the target used in each duplicate `isinstance` call (e.g., `obj` in
            // `isinstance(obj, int)`).
            let target = if let Expr::Call(ast::ExprCall {
                arguments: Arguments { args, .. },
                ..
            }) = &values[indices[0]]
            {
                args.first()
                    .expect("`isinstance` should have two arguments")
            } else {
                unreachable!("Indices should only contain `isinstance` calls")
            };
            let mut diagnostic = Diagnostic::new(
                DuplicateIsinstanceCall {
                    name: if let Expr::Name(ast::ExprName { id, .. }) = target {
                        Some(id.to_string())
                    } else {
                        None
                    },
                },
                expr.range(),
            );
            if !contains_effect(target, |id| checker.semantic().has_builtin_binding(id)) {
                // Grab the types used in each duplicate `isinstance` call (e.g., `int` and `str`
                // in `isinstance(obj, int) or isinstance(obj, str)`).
                let types: Vec<&Expr> = indices
                    .iter()
                    .map(|index| &values[*index])
                    .map(|expr| {
                        let Expr::Call(ast::ExprCall {
                            arguments: Arguments { args, .. },
                            ..
                        }) = expr
                        else {
                            unreachable!("Indices should only contain `isinstance` calls")
                        };
                        args.get(1).expect("`isinstance` should have two arguments")
                    })
                    .collect();

                // Generate a single `isinstance` call.
                let tuple = ast::ExprTuple {
                    // Flatten all the types used across the `isinstance` calls.
                    elts: types
                        .iter()
                        .flat_map(|value| {
                            if let Expr::Tuple(tuple) = value {
                                Left(tuple.iter())
                            } else {
                                Right(iter::once(*value))
                            }
                        })
                        .map(Clone::clone)
                        .collect(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                    parenthesized: true,
                };
                let isinstance_call = ast::ExprCall {
                    func: Box::new(
                        ast::ExprName {
                            id: Name::new_static("isinstance"),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                        }
                        .into(),
                    ),
                    arguments: Arguments {
                        args: Box::from([target.clone(), tuple.into()]),
                        keywords: Box::from([]),
                        range: TextRange::default(),
                    },
                    range: TextRange::default(),
                }
                .into();

                // Generate the combined `BoolOp`.
                let [first, .., last] = indices.as_slice() else {
                    unreachable!("Indices should have at least two elements")
                };
                let before = values.iter().take(*first).cloned();
                let after = values.iter().skip(last + 1).cloned();
                let bool_op = ast::ExprBoolOp {
                    op: BoolOp::Or,
                    values: before
                        .chain(iter::once(isinstance_call))
                        .chain(after)
                        .collect(),
                    range: TextRange::default(),
                }
                .into();
                let fixed_source = checker.generator().expr(&bool_op);

                // Populate the `Fix`. Replace the _entire_ `BoolOp`. Note that if we have
                // multiple duplicates, the fixes will conflict.
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    pad(fixed_source, expr.range(), checker.locator()),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn match_eq_target(expr: &Expr) -> Option<(&Name, &Expr)> {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
    }) = expr
    else {
        return None;
    };
    if **ops != [CmpOp::Eq] {
        return None;
    }
    let Expr::Name(ast::ExprName { id, .. }) = &**left else {
        return None;
    };
    let [comparator] = &**comparators else {
        return None;
    };
    if !comparator.is_name_expr() {
        return None;
    }
    Some((id, comparator))
}

/// SIM109
pub(crate) fn compare_with_tuple(checker: &mut Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        range: _,
    }) = expr
    else {
        return;
    };

    // Given `a == "foo" or a == "bar"`, we generate `{"a": [(0, "foo"), (1,
    // "bar")]}`.
    let mut id_to_comparators: BTreeMap<&Name, Vec<(usize, &Expr)>> = BTreeMap::new();
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
            .any(|expr| contains_effect(expr, |id| checker.semantic().has_builtin_binding(id)))
        {
            continue;
        }

        // Avoid removing comments.
        if checker
            .comment_ranges()
            .has_comments(expr, checker.locator())
        {
            continue;
        }

        // Create a `x in (a, b)` expression.
        let node = ast::ExprTuple {
            elts: comparators.into_iter().cloned().collect(),
            ctx: ExprContext::Load,
            range: TextRange::default(),
            parenthesized: true,
        };
        let node1 = ast::ExprName {
            id: id.clone(),
            ctx: ExprContext::Load,
            range: TextRange::default(),
        };
        let node2 = ast::ExprCompare {
            left: Box::new(node1.into()),
            ops: Box::from([CmpOp::In]),
            comparators: Box::from([node.into()]),
            range: TextRange::default(),
        };
        let in_expr = node2.into();
        let mut diagnostic = Diagnostic::new(
            CompareWithTuple {
                replacement: checker.generator().expr(&in_expr),
            },
            expr.range(),
        );
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
            let node = ast::ExprBoolOp {
                op: BoolOp::Or,
                values: iter::once(in_expr).chain(unmatched).collect(),
                range: TextRange::default(),
            };
            node.into()
        };
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(&in_expr),
            expr.range(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

/// SIM220
pub(crate) fn expr_and_not_expr(checker: &mut Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::And,
        values,
        range: _,
    }) = expr
    else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::Not,
            operand,
            range: _,
        }) = expr
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    if contains_effect(expr, |id| checker.semantic().has_builtin_binding(id)) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    ExprAndNotExpr {
                        name: id.to_string(),
                    },
                    expr.range(),
                );
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    "False".to_string(),
                    expr.range(),
                )));
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// SIM221
pub(crate) fn expr_or_not_expr(checker: &mut Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        range: _,
    }) = expr
    else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::Not,
            operand,
            range: _,
        }) = expr
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    if contains_effect(expr, |id| checker.semantic().has_builtin_binding(id)) {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    ExprOrNotExpr {
                        name: id.to_string(),
                    },
                    expr.range(),
                );
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    "True".to_string(),
                    expr.range(),
                )));
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

fn get_short_circuit_edit(
    expr: &Expr,
    range: TextRange,
    truthiness: bool,
    in_boolean_test: bool,
    generator: Generator,
) -> Edit {
    let content = if in_boolean_test {
        if truthiness {
            "True".to_string()
        } else {
            "False".to_string()
        }
    } else {
        generator.expr(expr)
    };
    Edit::range_replacement(
        if matches!(expr, Expr::Tuple(tuple) if !tuple.is_empty()) {
            format!("({content})")
        } else {
            content
        },
        range,
    )
}

fn is_short_circuit(
    expr: &Expr,
    expected_op: BoolOp,
    checker: &Checker,
) -> Option<(Edit, ContentAround)> {
    let Expr::BoolOp(ast::ExprBoolOp {
        op,
        values,
        range: _,
    }) = expr
    else {
        return None;
    };
    if *op != expected_op {
        return None;
    }
    let short_circuit_truthiness = match op {
        BoolOp::And => false,
        BoolOp::Or => true,
    };

    let mut furthest = expr;
    let mut edit = None;
    let mut remove = None;

    for (index, (value, next_value)) in values.iter().tuple_windows().enumerate() {
        // Keep track of the location of the furthest-right, truthy or falsey expression.
        let value_truthiness =
            Truthiness::from_expr(value, |id| checker.semantic().has_builtin_binding(id));
        let next_value_truthiness =
            Truthiness::from_expr(next_value, |id| checker.semantic().has_builtin_binding(id));

        // Keep track of the location of the furthest-right, non-effectful expression.
        if value_truthiness.is_unknown()
            && (!checker.semantic().in_boolean_test()
                || contains_effect(value, |id| checker.semantic().has_builtin_binding(id)))
        {
            furthest = next_value;
            continue;
        }

        // If the current expression is a constant, and it matches the short-circuit value, then
        // we can return the location of the expression. This should only trigger if the
        // short-circuit expression is the first expression in the list; otherwise, we'll see it
        // as `next_value` before we see it as `value`.
        if value_truthiness.into_bool() == Some(short_circuit_truthiness) {
            remove = Some(ContentAround::After);

            edit = Some(get_short_circuit_edit(
                value,
                TextRange::new(
                    parenthesized_range(
                        furthest.into(),
                        expr.into(),
                        checker.comment_ranges(),
                        checker.locator().contents(),
                    )
                    .unwrap_or(furthest.range())
                    .start(),
                    expr.end(),
                ),
                short_circuit_truthiness,
                checker.semantic().in_boolean_test(),
                checker.generator(),
            ));
            break;
        }

        // If the next expression is a constant, and it matches the short-circuit value, then
        // we can return the location of the expression.
        if next_value_truthiness.into_bool() == Some(short_circuit_truthiness) {
            remove = Some(if index + 1 == values.len() - 1 {
                ContentAround::Before
            } else {
                ContentAround::Both
            });
            edit = Some(get_short_circuit_edit(
                next_value,
                TextRange::new(
                    parenthesized_range(
                        furthest.into(),
                        expr.into(),
                        checker.comment_ranges(),
                        checker.locator().contents(),
                    )
                    .unwrap_or(furthest.range())
                    .start(),
                    expr.end(),
                ),
                short_circuit_truthiness,
                checker.semantic().in_boolean_test(),
                checker.generator(),
            ));
            break;
        }
    }

    match (edit, remove) {
        (Some(edit), Some(remove)) => Some((edit, remove)),
        _ => None,
    }
}

/// SIM222
pub(crate) fn expr_or_true(checker: &mut Checker, expr: &Expr) {
    if let Some((edit, remove)) = is_short_circuit(expr, BoolOp::Or, checker) {
        let mut diagnostic = Diagnostic::new(
            ExprOrTrue {
                expr: edit.content().unwrap_or_default().to_string(),
                remove,
            },
            edit.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(edit));
        checker.diagnostics.push(diagnostic);
    }
}

/// SIM223
pub(crate) fn expr_and_false(checker: &mut Checker, expr: &Expr) {
    if let Some((edit, remove)) = is_short_circuit(expr, BoolOp::And, checker) {
        let mut diagnostic = Diagnostic::new(
            ExprAndFalse {
                expr: edit.content().unwrap_or_default().to_string(),
                remove,
            },
            edit.range(),
        );
        diagnostic.set_fix(Fix::unsafe_edit(edit));
        checker.diagnostics.push(diagnostic);
    }
}
