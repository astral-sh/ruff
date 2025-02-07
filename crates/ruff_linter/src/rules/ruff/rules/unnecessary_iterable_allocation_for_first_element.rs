use std::borrow::Cow;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Arguments, Comprehension, Expr, Int};
use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::builtins::is_iterator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks the following constructs, all of which can be replaced by
/// `next(iter(...))`:
///
/// - `list(...)[0]`
/// - `tuple(...)[0]`
/// - `list(i for i in ...)[0]`
/// - `[i for i in ...][0]`
/// - `list(...).pop(0)`
///
/// ## Why is this bad?
/// Calling e.g. `list(...)` will create a new list of the entire collection,
/// which can be very expensive for large collections. If you only need the
/// first element of the collection, you can use `next(...)` or
/// `next(iter(...)` to lazily fetch the first element. The same is true for
/// the other constructs.
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
/// ## Fix safety
/// This rule's fix is marked as unsafe, as migrating from (e.g.) `list(...)[0]`
/// to `next(iter(...))` can change the behavior of your program in two ways:
///
/// 1. First, all above-mentioned constructs will eagerly evaluate the entire
///    collection, while `next(iter(...))` will only evaluate the first
///    element. As such, any side effects that occur during iteration will be
///    delayed.
/// 2. Second, accessing members of a collection via square bracket notation
///    `[0]` of the `pop()` function will raise `IndexError` if the collection
///    is empty, while `next(iter(...))` will raise `StopIteration`.
///
/// ## References
/// - [Iterators and Iterables in Python: Run Efficient Iterations](https://realpython.com/python-iterators-iterables/#when-to-use-an-iterator-in-python)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryIterableAllocationForFirstElement {
    iterable: SourceCodeSnippet,
}

impl AlwaysFixableViolation for UnnecessaryIterableAllocationForFirstElement {
    #[derive_message_formats]
    fn message(&self) -> String {
        let iterable = &self.iterable.truncated_display();
        format!("Prefer `next({iterable})` over single element slice")
    }

    fn fix_title(&self) -> String {
        let iterable = &self.iterable.truncated_display();
        format!("Replace with `next({iterable})`")
    }
}

/// RUF015
pub(crate) fn unnecessary_iterable_allocation_for_first_element(checker: &Checker, expr: &Expr) {
    let value = match expr {
        // Ex) `list(x)[0]`
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            if !is_zero(slice) {
                return;
            }
            value
        }
        // Ex) `list(x).pop(0)`
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
            if !arguments.keywords.is_empty() {
                return;
            }
            let [arg] = arguments.args.as_ref() else {
                return;
            };
            if !is_zero(arg) {
                return;
            }
            let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
                return;
            };
            if !matches!(attr.as_str(), "pop") {
                return;
            }
            value
        }
        _ => return,
    };

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
        expr.range(),
    );

    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
        format!("next({iterable})"),
        expr.range(),
    )));

    checker.report_diagnostic(diagnostic);
}

/// Check that the slice [`Expr`] is a slice of the first element (e.g., `x[0]`).
fn is_zero(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(Int::ZERO),
            ..
        })
    )
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
            let [arg] = &**args else {
                return None;
            };

            let builtin_function_name = semantic.resolve_builtin_symbol(func)?;
            if !matches!(builtin_function_name, "tuple" | "list") {
                return None;
            }

            match arg {
                Expr::Generator(ast::ExprGenerator {
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
                Expr::Call(ast::ExprCall { func, .. }) => IterationTarget {
                    range: arg.range(),
                    iterable: semantic
                        .resolve_builtin_symbol(func)
                        .is_some_and(is_iterator),
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
        Expr::List(ast::ExprList { elts, .. }) => {
            let [elt] = elts.as_slice() else {
                return None;
            };
            let Expr::Starred(ast::ExprStarred { value, .. }) = elt else {
                return None;
            };

            let iterable = if let ast::Expr::Call(ast::ExprCall { func, .. }) = &**value {
                semantic
                    .resolve_builtin_symbol(func)
                    .is_some_and(is_iterator)
            } else {
                false
            };
            IterationTarget {
                range: value.range(),
                iterable,
            }
        }
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
