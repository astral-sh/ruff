use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};

use ruff_python_ast::{
    self as ast, Arguments, AtomicNodeIndex, BoolOp, Expr, ExprContext, Identifier,
};
use ruff_python_semantic::{SemanticModel, analyze};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;
use crate::checkers::ast::Checker;
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
#[violation_metadata(stable_since = "v0.0.243")]
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

/// A matched `<name>.startswith(<arg>)` (or `endswith`) call.
struct StartsEndsWithCall<'a> {
    receiver_name: &'a str,
    attr: &'a str,
    arg: &'a Expr,
}

/// Returns `Some(...)` if `expr` is a call of the form
/// `<bare_name>.startswith(<arg>)` or `<bare_name>.endswith(<arg>)` with
/// exactly one positional argument and no keyword arguments.
fn match_starts_ends_with_call(expr: &Expr) -> Option<StartsEndsWithCall<'_>> {
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return None;
    };
    if !arguments.keywords.is_empty() {
        return None;
    }
    let [arg] = &*arguments.args else {
        return None;
    };
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };
    let attr = attr.as_str();
    if attr != "startswith" && attr != "endswith" {
        return None;
    }
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return None;
    };
    Some(StartsEndsWithCall {
        receiver_name: id.as_str(),
        attr,
        arg,
    })
}

/// Build `<receiver_name>.<attr>((<elements>...))`, flattening any tuple
/// literal element into the outer tuple so the result mirrors the shape
/// `str.startswith` / `str.endswith` actually accept.
fn build_starts_ends_with_tuple_call<'a>(
    receiver_name: &str,
    attr: &str,
    elements: impl IntoIterator<Item = &'a Expr>,
) -> Expr {
    let elts: Vec<Expr> = elements
        .into_iter()
        .flat_map(|value| {
            if let Expr::Tuple(tuple) = value {
                Left(tuple.iter())
            } else {
                Right(iter::once(value))
            }
        })
        .cloned()
        .collect();
    let tuple_arg = Expr::Tuple(ast::ExprTuple {
        elts,
        ctx: ExprContext::Load,
        range: TextRange::default(),
        node_index: AtomicNodeIndex::NONE,
        parenthesized: true,
    });
    Expr::Call(ast::ExprCall {
        func: Box::new(Expr::Attribute(ast::ExprAttribute {
            value: Box::new(Expr::Name(ast::ExprName {
                id: receiver_name.into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
                node_index: AtomicNodeIndex::NONE,
            })),
            attr: Identifier::new(attr.to_string(), TextRange::default()),
            ctx: ExprContext::Load,
            range: TextRange::default(),
            node_index: AtomicNodeIndex::NONE,
        })),
        arguments: Arguments {
            args: Box::from([tuple_arg]),
            keywords: Box::from([]),
            range: TextRange::default(),
            node_index: AtomicNodeIndex::NONE,
        },
        range: TextRange::default(),
        node_index: AtomicNodeIndex::NONE,
    })
}

/// PIE810 — `<x>.startswith(a) or <x>.startswith(b)` form.
pub(crate) fn multiple_starts_ends_with(checker: &Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        ..
    }) = expr
    else {
        return;
    };

    let mut duplicates: BTreeMap<(&str, &str), Vec<(usize, &Expr)>> = BTreeMap::new();
    for (index, call) in values.iter().enumerate() {
        let Some(matched) = match_starts_ends_with_call(call) else {
            continue;
        };
        // Skip `msg.startswith(x) or msg.startswith(y)` when one of the args
        // is already known to be a tuple — folding to `msg.startswith((x, y))`
        // would TypeError if `y` is a tuple.
        if is_bound_to_tuple(matched.arg, checker.semantic()) {
            continue;
        }
        duplicates
            .entry((matched.attr, matched.receiver_name))
            .or_default()
            .push((index, matched.arg));
    }

    for ((attr, receiver_name), entries) in duplicates {
        if entries.len() <= 1 {
            continue;
        }
        let mut diagnostic = checker.report_diagnostic(
            MultipleStartsEndsWith {
                attr: attr.to_string(),
            },
            expr.range(),
        );

        let new_call = build_starts_ends_with_tuple_call(
            receiver_name,
            attr,
            entries.iter().map(|(_, arg)| *arg),
        );

        // Regenerate the `or` chain with the folded call replacing the duplicates.
        let folded_indices: Vec<usize> = entries.iter().map(|(i, _)| *i).collect();
        let mut new_call = Some(new_call);
        let bool_op = Expr::BoolOp(ast::ExprBoolOp {
            op: BoolOp::Or,
            values: values
                .iter()
                .enumerate()
                .filter_map(|(index, elt)| {
                    if folded_indices.contains(&index) {
                        new_call.take()
                    } else {
                        Some(elt.clone())
                    }
                })
                .collect(),
            range: TextRange::default(),
            node_index: AtomicNodeIndex::NONE,
        });

        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(&bool_op),
            expr.range(),
        )));
    }
}

/// PIE810 — `any(<x>.startswith(p) for p in (...))` form.
pub(crate) fn multiple_starts_ends_with_any(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().match_builtin_expr(&call.func, "any") {
        return;
    }
    if !call.arguments.keywords.is_empty() {
        return;
    }
    let [first_arg] = &*call.arguments.args else {
        return;
    };

    let (element, generators) = match first_arg {
        Expr::Generator(ast::ExprGenerator {
            elt, generators, ..
        }) => (elt.as_ref(), generators),
        Expr::ListComp(ast::ExprListComp {
            elt, generators, ..
        }) => (elt.as_ref(), generators),
        Expr::SetComp(ast::ExprSetComp {
            elt, generators, ..
        }) => (elt.as_ref(), generators),
        _ => return,
    };

    let [comprehension] = generators.as_slice() else {
        return;
    };
    if comprehension.is_async || !comprehension.ifs.is_empty() {
        return;
    }
    let Expr::Name(target_name) = &comprehension.target else {
        return;
    };

    let Some(matched) = match_starts_ends_with_call(element) else {
        return;
    };
    let Expr::Name(arg_name) = matched.arg else {
        return;
    };
    if arg_name.id != target_name.id {
        return;
    }

    // Only fold a literal tuple, list, or set iterable: bare names can't be
    // proven to be a tuple at runtime, and other expressions could yield
    // non-strings or have side effects.
    let elts = match &comprehension.iter {
        Expr::Tuple(tuple) => &*tuple.elts,
        Expr::List(list) => &*list.elts,
        Expr::Set(set) => &*set.elts,
        _ => return,
    };

    // Mirror the `BoolOp` path's `is_bound_to_tuple` guard: if any iterable
    // element is a name that resolves to a tuple, folding would produce a
    // nested-tuple argument that `str.startswith` rejects at runtime.
    if elts
        .iter()
        .any(|elt| is_bound_to_tuple(elt, checker.semantic()))
    {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        MultipleStartsEndsWith {
            attr: matched.attr.to_string(),
        },
        call.range(),
    );

    let replacement_call =
        build_starts_ends_with_tuple_call(matched.receiver_name, matched.attr, elts.iter());

    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        checker.generator().expr(&replacement_call),
        call.range(),
    )));
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
