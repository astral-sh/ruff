use num_traits::ToPrimitive;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Ensures that instead of creating a new list and indexing into it to find the first element of a
/// collection (e.g., `list(...)[0]`), Python iterators are used.
///
/// Why is this bad?
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
    call: &Expr,
    slice: &Expr,
    subscript_range: &TextRange,
) {
    if !indexes_first_element(slice) {
        return;
    }
    let Some(iter_name) = get_iterable_name(checker, call) else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryListAllocationForFirstElement::new(iter_name.to_string()),
        *subscript_range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        let replacement = format!("next(iter({}))", iter_name);
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            replacement,
            *subscript_range,
        )));
    }

    checker.diagnostics.push(diagnostic);
}

/// Fetch the name of the iterable from a list expression.
fn get_iterable_name<'a>(checker: &mut Checker, expr: &'a Expr) -> Option<&'a String> {
    // Decompose.
    let name = match expr {
        Expr::Call(ast::ExprCall { func, args, .. }) => {
            let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
                return None;
            };
            if !(id == "list" && checker.semantic().is_builtin("list")) {
                return None;
            }

            let Some(Expr::Name(ast::ExprName { id: arg_name, .. })) = args.first() else {
                return None;
            };

            Some(arg_name)
        }
        Expr::ListComp(ast::ExprListComp { generators, .. }) => {
            // If there's more than 1 generator, we can't safely say that it fits the diagnostic conditions -
            // for example, `[i + j for i in x for j in y][0]`
            if generators.len() != 1 {
                return None;
            }

            let generator = &generators[0];
            let Expr::Name(ast::ExprName { id: arg_name, .. }) = &generator.iter else {
                return None;
            };

            Some(arg_name)
        }
        _ => None,
    };

    name
}

fn indexes_first_element(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(ast::ExprConstant { .. }) => get_index_value(expr) == Some(0i64),
        Expr::Slice(ast::ExprSlice { lower, upper, .. }) => {
            let lower_index = lower.as_ref().and_then(|l| get_index_value(&l));
            let upper_index = upper.as_ref().and_then(|u| get_index_value(&u));

            if lower_index.is_none() || lower_index == Some(0i64) {
                return upper_index == Some(1i64);
            } else {
                return false;
            }
        }
        _ => false,
    }
}

fn get_index_value(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(value),
            ..
        }) => value.to_i64(),
        _ => None,
    }
}
