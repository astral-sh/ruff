use std::iter;

use itertools::Either::{Left, Right};
use ruff_python_ast::{self as ast, Arguments, BoolOp, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::name::Name;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::{Edit, Fix, FixAvailability, Violation};
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
#[derive(ViolationMetadata)]
pub(crate) struct DuplicateIsinstanceCall {
    name: Option<String>,
}

impl Violation for DuplicateIsinstanceCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(name) = &self.name {
            format!("Multiple `isinstance` calls for `{name}`, merge into a single call")
        } else {
            "Multiple `isinstance` calls for expression, merge into a single call".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(if let Some(name) = &self.name {
            format!("Merge `isinstance` calls for `{name}`")
        } else {
            "Merge `isinstance` calls".to_string()
        })
    }
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
                node_index: _,
            },
        range: _,
        node_index: _,
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
pub(crate) fn duplicate_isinstance_call(checker: &Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        range: _,
        node_index: _,
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
            let mut diagnostic = checker.report_diagnostic(
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
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    parenthesized: true,
                };
                let isinstance_call = ast::ExprCall {
                    func: Box::new(
                        ast::ExprName {
                            id: Name::new_static("isinstance"),
                            ctx: ExprContext::Load,
                            range: TextRange::default(),
                            node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                        }
                        .into(),
                    ),
                    arguments: Arguments {
                        args: Box::from([target.clone(), tuple.into()]),
                        keywords: Box::from([]),
                        range: TextRange::default(),
                        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    },
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
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
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
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
        }
    }
}
