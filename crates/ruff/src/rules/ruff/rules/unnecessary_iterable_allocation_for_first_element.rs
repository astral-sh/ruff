use std::borrow::Cow;

use num_traits::Zero;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Comprehension, Constant, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::autofix::snippet::SourceCodeSnippet;
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
    iterable: SourceCodeSnippet,
}

impl AlwaysAutofixableViolation for UnnecessaryIterableAllocationForFirstElement {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryIterableAllocationForFirstElement { iterable } = self;
        let iterable = iterable.truncated_display();
        format!("Prefer `next({iterable})` over single element slice")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryIterableAllocationForFirstElement { iterable } = self;
        let iterable = iterable.truncated_display();
        format!("Replace with `next({iterable})`")
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

    if !is_head_slice(slice) {
        return;
    }

    let Some(target) = match_iteration_target(value, checker.semantic()) else {
        return;
    };
    let iterable = checker.locator().slice(target.range);
    let iterable = if target.iterable {
        Cow::Borrowed(iterable)
    } else {
        Cow::Owned(format!("iter({iterable})"))
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryIterableAllocationForFirstElement {
            iterable: SourceCodeSnippet::new(iterable.to_string()),
        },
        *range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            format!("next({iterable})"),
            *range,
        )));
    }

    checker.diagnostics.push(diagnostic);
}

/// Check that the slice [`Expr`] is a slice of the first element (e.g., `x[0]`).
fn is_head_slice(expr: &Expr) -> bool {
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(value),
        ..
    }) = expr
    {
        value.is_zero()
    } else {
        false
    }
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
fn match_iteration_target(expr: &Expr, semantic: &SemanticModel) -> Option<IterationTarget> {
    let result = match expr {
        Expr::Call(ast::ExprCall {
            func,
            arguments: Arguments { args, .. },
            ..
        }) => {
            let ast::ExprName { id, .. } = func.as_name_expr()?;

            if !matches!(id.as_str(), "tuple" | "list") {
                return None;
            }

            let [arg] = args.as_slice() else {
                return None;
            };

            if !semantic.is_builtin(id.as_str()) {
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
