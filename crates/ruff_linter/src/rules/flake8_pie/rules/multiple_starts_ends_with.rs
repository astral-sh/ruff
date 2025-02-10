use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};

use ruff_python_semantic::{analyze, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use ruff_python_ast::{self as ast, Arguments, BoolOp, Expr, ExprContext, Identifier};

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `startswith` or `endswith` calls on the same value with
/// different prefixes or suffixes.
///
/// ## Why is this bad?
/// The `startswith` and `endswith` methods accept tuples of prefixes or
/// suffixes respectively. Passing a tuple of prefixes or suffixes is more
/// efficient and readable than calling the method multiple times.
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
/// ## Fix safety
/// This rule's fix is unsafe, as in some cases, it will be unable to determine
/// whether the argument to an existing `.startswith` or `.endswith` call is a
/// tuple. For example, given `msg.startswith(x) or msg.startswith(y)`, if `x`
/// or `y` is a tuple, and the semantic model is unable to detect it as such,
/// the rule will suggest `msg.startswith((x, y))`, which will error at
/// runtime.
///
/// ## References
/// - [Python documentation: `str.startswith`](https://docs.python.org/3/library/stdtypes.html#str.startswith)
/// - [Python documentation: `str.endswith`](https://docs.python.org/3/library/stdtypes.html#str.endswith)
#[derive(ViolationMetadata)]
pub(crate) struct MultipleStartsEndsWith {
    attr: String,
}

impl AlwaysFixableViolation for MultipleStartsEndsWith {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MultipleStartsEndsWith { attr } = self;
        format!("Call `{attr}` once with a `tuple`")
    }

    fn fix_title(&self) -> String {
        let MultipleStartsEndsWith { attr } = self;
        format!("Merge into a single `{attr}` call")
    }
}

/// PIE810
pub(crate) fn multiple_starts_ends_with(checker: &Checker, expr: &Expr) {
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

        if !keywords.is_empty() {
            continue;
        }

        let [arg] = &**args else {
            continue;
        };

        let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
            continue;
        };
        if attr != "startswith" && attr != "endswith" {
            continue;
        }

        let Expr::Name(ast::ExprName { id: arg_name, .. }) = value.as_ref() else {
            continue;
        };

        // If the argument is bound to a tuple, skip it, since we don't want to suggest
        // `startswith((x, y))` where `x` or `y` are tuples. (Tuple literals are okay, since we
        // inline them below.)
        if is_bound_to_tuple(arg, checker.semantic()) {
            continue;
        }

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
                    args.first()
                        .unwrap_or_else(|| panic!("`{attr_name}` should have one argument"))
                })
                .collect();

            let node = Expr::Tuple(ast::ExprTuple {
                elts: words
                    .iter()
                    .flat_map(|value| {
                        if let Expr::Tuple(tuple) = value {
                            Left(tuple.iter())
                        } else {
                            Right(iter::once(*value))
                        }
                    })
                    .cloned()
                    .collect(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                parenthesized: true,
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
                    args: Box::from([node]),
                    keywords: Box::from([]),
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
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().expr(&bool_op),
                expr.range(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Returns `true` if the expression definitively resolves to a tuple (e.g., `x` in `x = (1, 2)`).
fn is_bound_to_tuple(arg: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = arg else {
        return false;
    };

    let Some(binding_id) = semantic.lookup_symbol(id.as_str()) else {
        return false;
    };

    let binding = semantic.binding(binding_id);

    analyze::typing::is_tuple(binding, semantic)
}
