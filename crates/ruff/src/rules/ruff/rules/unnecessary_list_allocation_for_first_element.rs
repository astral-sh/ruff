use num_traits::ToPrimitive;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::{self, Constant, Expr};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Ensures that instead of creating a new list and indexing into it to find the first element of a
/// collection (e.g., `list(...)[0]`), Python iterators are used.
///
/// ## Why is this bad?
/// Creating a new list of great size can involve significant memory/speed concerns. Python's `next(iter(...))`
/// pattern can be used in lieu of creating a new list. This pattern will lazily fetch the first
/// element of the collection, avoiding the memory overhead involved with new list allocation.
/// `next(iter(...))` also is much faster since the list itself doesn't get initialized at once.
///
/// ## Example
/// ```python
/// x = range(1000000000000)
/// return list(x)[0]
/// ```
///
/// Use instead:
/// ```python
/// x = range(1000000000000)
/// return next(iter(x))
/// ```
///
/// ## References
/// - [Iterators and Iterables in Python: Run Efficient
/// Iterations](https://realpython.com/python-iterators-iterables/#when-to-use-an-iterator-in-python)
#[violation]
pub struct UnnecessaryListAllocationForFirstElement {
    arg: String,
}

impl UnnecessaryListAllocationForFirstElement {
    pub(crate) fn new(arg: String) -> Self {
        Self { arg }
    }
}

impl AlwaysAutofixableViolation for UnnecessaryListAllocationForFirstElement {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Prefer `next(iter({}))` over `list({})[0]` or equivalent list comprehension",
            self.arg, self.arg
        )
    }

    fn autofix_title(&self) -> String {
        format!(
            "Convert `list({})[0]` or equivalent list comprehension call to `next(iter({}))`",
            self.arg, self.arg
        )
    }
}

/// RUF015
pub(crate) fn unnecessary_list_allocation_for_first_element(
    checker: &mut Checker,
    subscript: &Expr,
) {
    let Expr::Subscript(ast::ExprSubscript { value, slice, range, .. }) = subscript else {
        return;
    };

    let (indexes_first_element, in_slice) = classify_subscript(slice);
    if !indexes_first_element {
        return;
    }
    let Some(iter_name) = get_iterable_name(checker, value) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryListAllocationForFirstElement::new(iter_name.to_string()),
        *range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        let replacement = if in_slice {
            format!("[next(iter({iter_name}))]")
        } else {
            format!("next(iter({iter_name}))")
        };

        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(replacement, *range)));
    }

    checker.diagnostics.push(diagnostic);
}

/// Fetch the name of the iterable from a list expression if the expression returns an unmodified list
/// which can be sliced into.
fn get_iterable_name<'a>(checker: &mut Checker, expr: &'a Expr) -> Option<&'a str> {
    match expr {
        Expr::Call(ast::ExprCall { func, args, .. }) => {
            let Some(id) = get_name_id(func.as_ref()) else {
                return None;
            };
            if !(id == "list" && checker.semantic().is_builtin("list")) {
                return None;
            }

            let Some(Expr::Name(ast::ExprName { id: arg_name, .. })) = args.first() else {
                return None;
            };

            Some(arg_name.as_str())
        }
        Expr::ListComp(ast::ExprListComp {
            elt, generators, ..
        }) => {
            // If the `elt` field is anything other than a [`Expr::Name`], we can't be sure that it
            // doesn't modify the elements of the underlying iterator - for example, `[i + 1 for i in x][0]`.
            if !matches!(elt.as_ref(), Expr::Name(ast::ExprName { .. })) {
                return None;
            }

            // If there's more than 1 generator, we can't safely say that it fits the diagnostic conditions -
            // for example, `[(i, j) for i in x for j in y][0]`.
            if generators.len() != 1 {
                return None;
            }

            let generator = &generators[0];
            // Ignore if there's an `if` statement in the comprehension, since it filters the list.
            if !generator.ifs.is_empty() {
                return None;
            }

            let Some(arg_name) = get_name_id(&generator.iter) else {
                return None;
            };
            Some(arg_name)
        }
        _ => None,
    }
}

/// Check that the slice [`Expr`] is functionally equivalent to slicing into the first element. The
/// first `bool` checks that the element is in fact first, the second checks if it's a slice or an
/// index.
fn classify_subscript(expr: &Expr) -> (bool, bool) {
    match expr {
        Expr::Constant(ast::ExprConstant { .. }) => {
            let effective_index = get_effective_index(expr);
            (acts_as_zero(effective_index), false)
        }
        Expr::Slice(ast::ExprSlice {
            step: step_value,
            lower: lower_index,
            upper: upper_index,
            ..
        }) => {
            let lower = lower_index.as_ref().and_then(|l| get_effective_index(l));
            let upper = upper_index.as_ref().and_then(|u| get_effective_index(u));
            let step = step_value.as_ref().and_then(|s| get_effective_index(s));

            let in_slice = upper.is_some() || step.is_some();
            if acts_as_zero(lower) {
                if upper.unwrap_or(i64::MAX) > step.unwrap_or(1i64) {
                    return (false, in_slice);
                }

                return (true, in_slice);
            }

            (false, in_slice)
        }
        _ => (false, false),
    }
}

fn get_effective_index(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(value),
            ..
        }) => value.to_i64(),
        _ => None,
    }
}

fn get_name_id(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => Some(id),
        _ => None,
    }
}

fn acts_as_zero(i: Option<i64>) -> bool {
    i.is_none() || i == Some(0i64)
}
