use ruff_python_ast::{Builder, ExprBoolOpFields, ExprFields, ExprKind, build};
use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};

use ruff_python_semantic::{SemanticModel, analyze};
use ruff_text_size::{Ranged, TextRange};

use ruff_python_ast::{BoolOp, Expr};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
use crate::{Edit, Fix};

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
#[violation_metadata(stable_since = "v0.0.243", category = Category::Complexity)]
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
pub(crate) fn multiple_starts_ends_with(checker: &Checker, &expr: &Expr<'_>) {
    let ExprFields::BoolOp(ExprBoolOpFields {
        op: BoolOp::Or,
        values,
        range: _,
        node: _,
    }) = expr.fields()
    else {
        return;
    };

    let mut duplicates = BTreeMap::new();
    for (index, call) in values.iter().enumerate() {
        let ExprKind::Call(call_node) = call.classify() else {
            continue;
        };
        let func = call_node.func();
        let arguments = call_node.arguments();
        let args = arguments.args();
        let keywords = arguments.keywords();

        if !keywords.is_empty() {
            continue;
        }

        let Some([arg]) = args.as_array() else {
            continue;
        };

        let ExprKind::Attribute(attribute) = func.classify() else {
            continue;
        };
        let value = attribute.value();
        let attr = attribute.attr();
        if attr != "startswith" && attr != "endswith" {
            continue;
        }

        let ExprKind::Name(name) = value.classify() else {
            continue;
        };
        let arg_name = name.id();

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
            let mut diagnostic = checker.report_diagnostic(
                MultipleStartsEndsWith {
                    attr: attr_name.to_string(),
                },
                expr.range(),
            );
            let words: Vec<Expr<'_>> = indices
                .iter()
                .map(|index| values.at(*index))
                .map(|expr| {
                    let ExprKind::Call(call_node) = expr.classify() else {
                        unreachable!(
                            "{}",
                            format!("Indices should only contain `{attr_name}` calls")
                        )
                    };
                    let arguments = call_node.arguments();
                    let args = arguments.args();
                    args.first()
                        .unwrap_or_else(|| panic!("`{attr_name}` should have one argument"))
                })
                .collect();

            let mut builder = Builder::default();
            let words = builder.copy_nodes(words.into_iter().flat_map(|value| {
                if let ExprKind::Tuple(tuple) = value.classify() {
                    Left(tuple.iter())
                } else {
                    Right(iter::once(value))
                }
            }));
            let words = syntax::tuple(&mut builder, words);
            let receiver = syntax::name(&mut builder, arg_name);
            let args = builder.list([words]);
            let call = syntax::call_method(&mut builder, receiver, attr_name, args);

            let first = indices[0];
            let values = builder.map_list(
                (0..values.len()).filter(|index| *index == first || !indices.contains(index)),
                |builder, index| {
                    if index == first {
                        call
                    } else {
                        builder.copy_node(values.at(index))
                    }
                },
            );
            let expression = builder.add(build::ExprBoolOp {
                op: BoolOp::Or,
                values,
                range: TextRange::default(),
            });
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().expr(builder.get(expression).into()),
                expr.range(),
            )));
        }
    }
}

/// Returns `true` if the expression definitively resolves to a tuple (e.g., `x` in `x = (1, 2)`).
fn is_bound_to_tuple(arg: Expr<'_>, semantic: &SemanticModel) -> bool {
    let ExprKind::Name(name) = arg.classify() else {
        return false;
    };
    let id = name.id();

    let Some(binding_id) = semantic.lookup_symbol(id.as_str()).binding_id() else {
        return false;
    };

    let binding = semantic.binding(binding_id);

    analyze::typing::is_tuple(binding, semantic)
}
