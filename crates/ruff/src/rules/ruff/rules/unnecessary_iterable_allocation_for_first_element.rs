use num_bigint::BigInt;
use num_traits::{One, Zero};
use rustpython_parser::ast::{self, Comprehension, Constant, Expr, ExprSubscript};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `list(...)[0]` that can be replaced with
/// `next(iter(...))`.
///
/// ## Why is this bad?
/// Calling `list(...)` will create a new list of the entire collection, which
/// can be very expensive for large collections. If you only need the first
/// element of the collection, you can use `next(iter(...))` to lazily fetch
/// the first element without creating a new list.
///
/// Note that migrating from `list(...)[0]` to `next(iter(...))` can change
/// the behavior of your program in two ways:
///
/// 1. First, `list(...)` will eagerly evaluate the entire collection, while
///    `next(iter(...))` will only evaluate the first element. As such, any
///    side effects that occur during iteration will be delayed.
/// 2. Second, `list(...)[0]` will raise `IndexError` if the collection is
///   empty, while `next(iter(...))` will raise `StopIteration`.
///
/// ## Example
/// ```python
/// head = list(range(1000000000000))[0]
/// ```
///
/// Use instead:
/// ```python
/// head = next(iter(range(1000000000000)))
/// ```
///
/// ## References
/// - [Iterators and Iterables in Python: Run Efficient Iterations](https://realpython.com/python-iterators-iterables/#when-to-use-an-iterator-in-python)
#[violation]
pub(crate) struct UnnecessaryIterableAllocationForFirstElement {
    iterable: String,
    subscript_kind: HeadSubscriptKind,
}

impl AlwaysAutofixableViolation for UnnecessaryIterableAllocationForFirstElement {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryIterableAllocationForFirstElement {
            iterable,
            subscript_kind,
        } = self;
        match subscript_kind {
            HeadSubscriptKind::Index => {
                format!("Prefer `next(iter({iterable}))` over `list({iterable})[0]`")
            }
            HeadSubscriptKind::Slice => {
                format!("Prefer `[next(iter({iterable}))]` over `list({iterable})[:1]`")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryIterableAllocationForFirstElement {
            iterable,
            subscript_kind,
        } = self;
        match subscript_kind {
            HeadSubscriptKind::Index => format!("Replace with `next(iter({iterable}))`"),
            HeadSubscriptKind::Slice => format!("Replace with `[next(iter({iterable}))]"),
        }
    }
}

/// RUF015
pub(crate) fn unnecessary_iterable_allocation_for_first_element(
    checker: &mut Checker,
    subscript: &ExprSubscript,
) {
    let ast::ExprSubscript {
        value,
        slice,
        range,
        ..
    } = subscript;

    let Some(subscript_kind) = classify_subscript(slice) else {
        return;
    };

    let Some(iterable) = iterable_name(value, checker.semantic()) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryIterableAllocationForFirstElement {
            iterable: iterable.to_string(),
            subscript_kind,
        },
        *range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        let replacement = match subscript_kind {
            HeadSubscriptKind::Index => format!("next(iter({iterable}))"),
            HeadSubscriptKind::Slice => format!("[next(iter({iterable}))]"),
        };
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(replacement, *range)));
    }

    checker.diagnostics.push(diagnostic);
}

/// A subscript slice that represents the first element of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadSubscriptKind {
    /// The subscript is an index (e.g., `[0]`).
    Index,
    /// The subscript is a slice (e.g., `[:1]`).
    Slice,
}

/// Check that the slice [`Expr`] is functionally equivalent to slicing into the first element. The
/// first `bool` checks that the element is in fact first, the second checks if it's a slice or an
/// index.
fn classify_subscript(expr: &Expr) -> Option<HeadSubscriptKind> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(value),
            ..
        }) if value.is_zero() => Some(HeadSubscriptKind::Index),
        Expr::Slice(ast::ExprSlice {
            step, lower, upper, ..
        }) => {
            // Avoid, e.g., `list(...)[:2]`
            let upper = upper.as_ref()?;
            let upper = as_int(upper)?;
            if !upper.is_one() {
                return None;
            }

            // Avoid, e.g., `list(...)[2:]`.
            if let Some(lower) = lower.as_ref() {
                let lower = as_int(lower)?;
                if !lower.is_zero() {
                    return None;
                }
            }

            // Avoid, e.g., `list(...)[::-1]`
            if let Some(step) = step.as_ref() {
                let step = as_int(step)?;
                if step < upper {
                    return None;
                }
            }

            Some(HeadSubscriptKind::Slice)
        }
        _ => None,
    }
}

/// Fetch the name of the iterable from an expression if the expression returns an unmodified list
/// which can be sliced into.
fn iterable_name<'a>(expr: &'a Expr, model: &SemanticModel) -> Option<&'a str> {
    match expr {
        Expr::Call(ast::ExprCall { func, args, .. }) => {
            let ast::ExprName { id, .. } = func.as_name_expr()?;

            if !matches!(id.as_str(), "tuple" | "list") {
                return None;
            }

            if !model.is_builtin(id.as_str()) {
                return None;
            }

            match args.first() {
                Some(Expr::Name(ast::ExprName { id: arg_name, .. })) => Some(arg_name.as_str()),
                Some(Expr::GeneratorExp(ast::ExprGeneratorExp {
                    elt, generators, ..
                })) => generator_iterable(elt, generators),
                _ => None,
            }
        }
        Expr::ListComp(ast::ExprListComp {
            elt, generators, ..
        }) => generator_iterable(elt, generators),
        _ => None,
    }
}

/// Given a comprehension, returns the name of the iterable over which it iterates, if it's
/// a simple comprehension (e.g., `x` for `[i for i in x]`).
fn generator_iterable<'a>(elt: &'a Expr, generators: &'a Vec<Comprehension>) -> Option<&'a str> {
    // If the `elt` field is anything other than a [`Expr::Name`], we can't be sure that it
    // doesn't modify the elements of the underlying iterator (e.g., `[i + 1 for i in x][0]`).
    if !elt.is_name_expr() {
        return None;
    }

    // If there's more than 1 generator, we can't safely say that it fits the diagnostic conditions
    // (e.g., `[(i, j) for i in x for j in y][0]`).
    let [generator] = generators.as_slice() else {
        return None;
    };

    // Ignore if there's an `if` statement in the comprehension, since it filters the list.
    if !generator.ifs.is_empty() {
        return None;
    }

    let ast::ExprName { id, .. } = generator.iter.as_name_expr()?;
    Some(id.as_str())
}

/// If an expression is a constant integer, returns the value of that integer; otherwise,
/// returns `None`.
fn as_int(expr: &Expr) -> Option<&BigInt> {
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(value),
        ..
    }) = expr
    {
        Some(value)
    } else {
        None
    }
}
