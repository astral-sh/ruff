use ruff_diagnostics::{Violation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Parameters, Stmt, identifier::Identifier};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions that include too many positional arguments.
///
/// ## Why is this bad?
/// The position of arguments that are not defined as keyword-only
/// becomes part of the function’s API. This encourages consumers of the API
/// to specify arguments positionally, making their code harder to read,
/// and complicating refactoring of the API, such as deprecating a parameter.
///
/// ## Example
/// ```python
/// def plot(x, y, z, color, mark, add_trendline):
///     ...
/// 
/// plot(1, 2, 3, 'r', '*', True)
/// ```
///
/// Use instead:
/// ```python
/// def plot(x, y, z, *, color, mark, add_trendline):
///     ...
/// 
/// plot(1, 2, 3, color='r', mark='*', add_trendline=True)
/// ```
#[violation]
pub struct TooManyPositional {
    c_pos: usize,
    max_pos: usize,
}

impl Violation for TooManyPositional {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyPositional { c_pos, max_pos } = self;
        format!("Too many positional arguments: ({c_pos}/{max_pos})")
    }
}

/// PLRR0917
pub(crate) fn too_many_positional(checker: &mut Checker, parameters: &Parameters, stmt: &Stmt) {
    let num_positional = parameters
        .args
        .iter()
        .chain(&parameters.posonlyargs)
        .filter(|arg| {
            !checker
                .settings
                .dummy_variable_rgx
                .is_match(&arg.parameter.name)
        })
        .count();
    if num_positional > checker.settings.pylint.max_pos_args {
        checker.diagnostics.push(Diagnostic::new(
            TooManyPositional {
                c_pos: num_positional,
                max_pos: checker.settings.pylint.max_pos_args,
            },
            stmt.identifier(),
        ));
    }
}
