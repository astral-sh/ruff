use rustpython_parser::ast::{Arguments, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for mutable data structures used as argument defaults.
///
/// ## Why is this bad?
/// Argument defaults are evaluated only once, when the function is defined.
/// The same mutable data structure is shared across all calls to the function.
/// If the function modifies the data structure, the changes will persist
/// across calls, which can lead to unexpected behavior.
///
/// Instead of using mutable data structures as argument defaults, use
/// immutable data structures or `None` as defaults, and create a new mutable
/// data structure inside the function body.
///
/// ## Example
/// ```python
/// def add_to_list(item, some_list=[]):
///     some_list.append(item)
///     return some_list
///
///
/// l1 = add_to_list(0)  # [0]
/// l2 = add_to_list(1)  # [0, 1]
/// ```
///
/// Use instead:
/// ```python
/// def add_to_list(item, some_list=None):
///     if some_list is None:
///         some_list = []
///     some_list.append(item)
///     return some_list
///
///
/// l1 = add_to_list(0)  # [0]
/// l2 = add_to_list(1)  # [1]
/// ```
///
/// ## References
/// - [Python documentation: Default Argument Values](https://docs.python.org/3/tutorial/controlflow.html#default-argument-values)
#[violation]
pub struct MutableArgumentDefault;

impl Violation for MutableArgumentDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
}

/// B006
pub(crate) fn mutable_argument_default(checker: &mut Checker, arguments: &Arguments) {
    // Scan in reverse order to right-align zip().
    for (arg, default) in arguments
        .kwonlyargs
        .iter()
        .rev()
        .zip(arguments.kw_defaults.iter().rev())
        .chain(
            arguments
                .args
                .iter()
                .rev()
                .chain(arguments.posonlyargs.iter().rev())
                .zip(arguments.defaults.iter().rev()),
        )
    {
        if is_mutable_expr(default, checker.semantic())
            && !arg.annotation.as_ref().map_or(false, |expr| {
                is_immutable_annotation(expr, checker.semantic())
            })
        {
            checker
                .diagnostics
                .push(Diagnostic::new(MutableArgumentDefault, default.range()));
        }
    }
}
