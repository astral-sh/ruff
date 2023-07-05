use num_traits::ToPrimitive;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
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
) {
    let Some(ListComponents { func_range, subscript_range, iter_name }) = decompose_list_expr(checker, call, slice) else {
        return;
    };

    let range = TextRange::at(func_range.start(), func_range.len() + subscript_range.len());
    let mut diagnostic = Diagnostic::new(
        UnnecessaryListAllocationForFirstElement::new(iter_name.to_string()),
        range,
    );

    if checker.patch(diagnostic.kind.rule()) {
        let replacement = format!("next(iter({}))", iter_name);
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(replacement, range)));
    }

    checker.diagnostics.push(diagnostic);
}

/// Lighweight record struct that represents the components required for a list creation.
struct ListComponents<'a> {
    /// The [`TextRange`] for the actual list creation - either `list(x)` or `[i for i in x]`
    func_range: &'a TextRange,
    /// The subscript's (e.g., `[0]`) [`TextRange`]
    subscript_range: &'a TextRange,
    /// The name of the iterable - the "x" in `list(x)` and `[i for i in x]`
    iter_name: &'a str,
}

impl<'a> ListComponents<'a> {
    fn new(func_range: &'a TextRange, slice_range: &'a TextRange, arg_name: &'a str) -> Self {
        Self {
            func_range,
            subscript_range: slice_range,
            iter_name: arg_name,
        }
    }
}

// Decompose an [`Expr`] into the parts relevant for the diagnostic. If the [`Expr`] in question
// isn't a list, return None
fn decompose_list_expr<'a>(
    checker: &mut Checker,
    expr: &'a Expr,
    slice: &'a Expr,
) -> Option<ListComponents<'a>> {
    // Ensure slice is at 0
    let Expr::Constant(ast::ExprConstant{ value: Constant::Int(slice_index), range: slice_range, .. }) = slice else {
        return None;
    };
    if slice_index.to_i64() != Some(0i64) {
        return None;
    }

    // Decompose.
    let list_components = match expr {
        Expr::Call(ast::ExprCall {
            func, range, args, ..
        }) => {
            let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
                return None;
            };
            if !(id == "list" && checker.semantic().is_builtin("list")) {
                return None;
            }

            let Some(Expr::Name(ast::ExprName { id: arg_name, .. })) = args.first() else {
                return None;
            };

            Some(ListComponents::new(range, slice_range, arg_name))
        }
        Expr::ListComp(ast::ExprListComp {
            range, generators, ..
        }) => {
            // If there's more than 1 generator, we can't safely say that it's invalid. For
            // example, `[(i, j) for i in x for j in y][0]`
            if generators.len() != 1 {
                return None;
            }

            let generator = &generators[0];
            let Expr::Name(ast::ExprName { id: arg_name, .. }) = &generator.iter else {
                return None;
            };

            Some(ListComponents::new(range, slice_range, arg_name.as_str()))
        }
        _ => None,
    };

    list_components
}
