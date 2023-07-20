use std::borrow::Cow;

use num_bigint::BigInt;
use num_traits::{One, Zero};
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{self, Comprehension, Constant, Expr, Ranged};
use unicode_width::UnicodeWidthStr;

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
/// element of the collection, you can use `next(...)` or `next(iter(...)` to
/// lazily fetch the first element.
///
/// Note that migrating from `list(...)[0]` to `next(iter(...))` can change
/// the behavior of your program in two ways:
///
/// 1. First, `list(...)` will eagerly evaluate the entire collection, while
///    `next(iter(...))` will only evaluate the first element. As such, any
///    side effects that occur during iteration will be delayed.
/// 2. Second, `list(...)[0]` will raise `IndexError` if the collection is
///    empty, while `next(iter(...))` will raise `StopIteration`.
///
/// ## Example
/// ```python
/// head = list(x)[0]
/// head = [x * x for x in range(10)][0]
/// ```
///
/// Use instead:
/// ```python
/// head = next(iter(x))
/// head = next(x * x for x in range(10))
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
        let iterable = Self::truncate(iterable);
        match subscript_kind {
            HeadSubscriptKind::Index => {
                format!("Prefer `next({iterable})` over single element slice")
            }
            HeadSubscriptKind::Slice => {
                format!("Prefer `[next({iterable})]` over single element slice")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryIterableAllocationForFirstElement {
            iterable,
            subscript_kind,
        } = self;
        let iterable = Self::truncate(iterable);
        match subscript_kind {
            HeadSubscriptKind::Index => format!("Replace with `next({iterable})`"),
            HeadSubscriptKind::Slice => format!("Replace with `[next({iterable})]"),
        }
    }
}

impl UnnecessaryIterableAllocationForFirstElement {
    /// If the iterable is too long, or spans multiple lines, truncate it.
    fn truncate(iterable: &str) -> &str {
        if iterable.width() > 40 || iterable.contains(['\r', '\n']) {
            "..."
        } else {
            iterable
        }
    }
}

/// RUF015
pub(crate) fn unnecessary_iterable_allocation_for_first_element(
    checker: &mut Checker,
    subscript: &ast::ExprSubscript,
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

    let Some(target) = match_iteration_target(value, checker.semantic()) else {
        return;
    };
    let iterable = checker.locator.slice(target.range);
    let iterable = if target.iterable {
        Cow::Borrowed(iterable)
    } else {
        Cow::Owned(format!("iter({iterable})"))
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
            HeadSubscriptKind::Index => format!("next({iterable})"),
            HeadSubscriptKind::Slice => format!("[next({iterable})]"),
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
    let result = match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(value),
            ..
        }) if value.is_zero() => HeadSubscriptKind::Index,
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

            HeadSubscriptKind::Slice
        }
        _ => return None,
    };

    Some(result)
}

#[derive(Debug)]
struct IterationTarget {
    /// The [`TextRange`] of the target.
    range: TextRange,
    /// Whether the target is an iterable (e.g., a generator). If not, the target must be wrapped
    /// in `iter(...)` prior to calling `next(...)`.
    iterable: bool,
}

/// Return the [`IterationTarget`] of an expression, if the expression can be sliced into (i.e.,
/// is a list comprehension, or call to `list` or `tuple`).
///
/// For example, given `list(x)`, returns the range of `x`. Given `[x * x for x in y]`, returns the
/// range of `x * x for x in y`.
///
/// As a special-case, given `[x for x in y]`, returns the range of `y` (rather than the
/// redundant comprehension).
fn match_iteration_target(expr: &Expr, model: &SemanticModel) -> Option<IterationTarget> {
    let result = match expr {
        Expr::Call(ast::ExprCall { func, args, .. }) => {
            let ast::ExprName { id, .. } = func.as_name_expr()?;

            if !matches!(id.as_str(), "tuple" | "list") {
                return None;
            }

            let [arg] = args.as_slice() else {
                return None;
            };

            if !model.is_builtin(id.as_str()) {
                return None;
            }

            match arg {
                Expr::GeneratorExp(ast::ExprGeneratorExp {
                    elt, generators, ..
                }) => match match_simple_comprehension(elt, generators) {
                    Some(range) => IterationTarget {
                        range,
                        iterable: false,
                    },
                    None => IterationTarget {
                        range: arg.range(),
                        iterable: true,
                    },
                },
                Expr::ListComp(ast::ExprListComp {
                    elt, generators, ..
                }) => match match_simple_comprehension(elt, generators) {
                    Some(range) => IterationTarget {
                        range,
                        iterable: false,
                    },
                    None => IterationTarget {
                        range: arg
                            .range()
                            // Remove the `[`
                            .add_start(TextSize::from(1))
                            // Remove the `]`
                            .sub_end(TextSize::from(1)),
                        iterable: true,
                    },
                },
                _ => IterationTarget {
                    range: arg.range(),
                    iterable: false,
                },
            }
        }

        Expr::ListComp(ast::ExprListComp {
            elt, generators, ..
        }) => match match_simple_comprehension(elt, generators) {
            Some(range) => IterationTarget {
                range,
                iterable: false,
            },
            None => IterationTarget {
                range: expr
                    .range()
                    // Remove the `[`
                    .add_start(TextSize::from(1))
                    // Remove the `]`
                    .sub_end(TextSize::from(1)),
                iterable: true,
            },
        },

        _ => return None,
    };

    Some(result)
}

/// Returns the [`Expr`] target for a comprehension, if the comprehension is "simple"
/// (e.g., `x` for `[i for i in x]`).
fn match_simple_comprehension(elt: &Expr, generators: &[Comprehension]) -> Option<TextRange> {
    let [generator @ Comprehension {
        is_async: false, ..
    }] = generators
    else {
        return None;
    };

    // Ignore if there's an `if` statement in the comprehension, since it filters the list.
    if !generator.ifs.is_empty() {
        return None;
    }

    // Verify that the generator is, e.g. `i for i in x`, as opposed to `i for j in x`.
    let elt = elt.as_name_expr()?;
    let target = generator.target.as_name_expr()?;
    if elt.id != target.id {
        return None;
    }

    Some(generator.iter.range())
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
