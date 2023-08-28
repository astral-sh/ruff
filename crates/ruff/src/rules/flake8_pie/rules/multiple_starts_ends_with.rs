use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};

use ruff_text_size::{Ranged, TextRange};

use ruff_python_ast::{self as ast, Arguments, BoolOp, Expr, ExprContext, Identifier};

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `startswith` or `endswith` calls on the same value with
/// different prefixes or suffixes.
///
/// ## Why is this bad?
/// The `startswith` and `endswith` methods accept tuples of prefixes or
/// suffixes respectively. Passing a tuple of prefixes or suffixes is more
/// more efficient and readable than calling the method multiple times.
///
/// ## Example
/// ```python
/// msg = "Hello, world!"
/// if msg.startswith("Hello") or msg.startswith("Hi"):
///     print("Greetings!")
/// ```
///
/// Use instead:
/// ```python
/// msg = "Hello, world!"
/// if msg.startswith(("Hello", "Hi")):
///     print("Greetings!")
/// ```
///
/// ## References
/// - [Python documentation: `str.startswith`](https://docs.python.org/3/library/stdtypes.html#str.startswith)
/// - [Python documentation: `str.endswith`](https://docs.python.org/3/library/stdtypes.html#str.endswith)
#[violation]
pub struct MultipleStartsEndsWith {
    attr: String,
}

impl AlwaysAutofixableViolation for MultipleStartsEndsWith {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MultipleStartsEndsWith { attr } = self;
        format!("Call `{attr}` once with a `tuple`")
    }

    fn autofix_title(&self) -> String {
        let MultipleStartsEndsWith { attr } = self;
        format!("Merge into a single `{attr}` call")
    }
}

/// PIE810
pub(crate) fn multiple_starts_ends_with(checker: &mut Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        range: _,
    }) = expr
    else {
        return;
    };

    let mut duplicates = BTreeMap::new();
    for (index, call) in values.iter().enumerate() {
        let Expr::Call(ast::ExprCall {
            func,
            arguments:
                Arguments {
                    args,
                    keywords,
                    range: _,
                },
            range: _,
        }) = &call
        else {
            continue;
        };

        if !(args.len() == 1 && keywords.is_empty()) {
            continue;
        }

        let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
            continue;
        };
        if attr != "startswith" && attr != "endswith" {
            continue;
        }

        let Expr::Name(ast::ExprName { id: arg_name, .. }) = value.as_ref() else {
            continue;
        };

        duplicates
            .entry((attr.as_str(), arg_name.as_str()))
            .or_insert_with(Vec::new)
            .push(index);
    }

    // Generate a `Diagnostic` for each duplicate.
    for ((attr_name, arg_name), indices) in duplicates {
        if indices.len() > 1 {
            let mut diagnostic = Diagnostic::new(
                MultipleStartsEndsWith {
                    attr: attr_name.to_string(),
                },
                expr.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let words: Vec<&Expr> = indices
                    .iter()
                    .map(|index| &values[*index])
                    .map(|expr| {
                        let Expr::Call(ast::ExprCall {
                            func: _,
                            arguments:
                                Arguments {
                                    args,
                                    keywords: _,
                                    range: _,
                                },
                            range: _,
                        }) = expr
                        else {
                            unreachable!(
                                "{}",
                                format!("Indices should only contain `{attr_name}` calls")
                            )
                        };
                        args.get(0)
                            .unwrap_or_else(|| panic!("`{attr_name}` should have one argument"))
                    })
                    .collect();

                let node = Expr::Tuple(ast::ExprTuple {
                    elts: words
                        .iter()
                        .flat_map(|value| {
                            if let Expr::Tuple(ast::ExprTuple { elts, .. }) = value {
                                Left(elts.iter())
                            } else {
                                Right(iter::once(*value))
                            }
                        })
                        .map(Clone::clone)
                        .collect(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });
                let node1 = Expr::Name(ast::ExprName {
                    id: arg_name.into(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });
                let node2 = Expr::Attribute(ast::ExprAttribute {
                    value: Box::new(node1),
                    attr: Identifier::new(attr_name.to_string(), TextRange::default()),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                });
                let node3 = Expr::Call(ast::ExprCall {
                    func: Box::new(node2),
                    arguments: Arguments {
                        args: vec![node],
                        keywords: vec![],
                        range: TextRange::default(),
                    },
                    range: TextRange::default(),
                });
                let call = node3;

                // Generate the combined `BoolOp`.
                let mut call = Some(call);
                let node = Expr::BoolOp(ast::ExprBoolOp {
                    op: BoolOp::Or,
                    values: values
                        .iter()
                        .enumerate()
                        .filter_map(|(index, elt)| {
                            if indices.contains(&index) {
                                std::mem::take(&mut call)
                            } else {
                                Some(elt.clone())
                            }
                        })
                        .collect(),
                    range: TextRange::default(),
                });
                let bool_op = node;
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    checker.generator().expr(&bool_op),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
