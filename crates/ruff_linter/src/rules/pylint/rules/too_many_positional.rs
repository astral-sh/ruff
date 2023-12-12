use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, identifier::Identifier};
use ruff_python_semantic::analyze::visibility;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions that include too many positional arguments.
///
/// By default, this rule allows up to five arguments, as configured by the
/// [`pylint.max-positional-args`] option.
///
/// ## Why is this bad?
/// Functions with many arguments are harder to understand, maintain, and call.
/// This is especially true for functions with many positional arguments, as
/// providing arguments positionally is more error-prone and less clear to
/// readers than providing arguments by name.
///
/// Consider refactoring functions with many arguments into smaller functions
/// with fewer arguments, using objects to group related arguments, or
/// migrating to keyword-only arguments.
///
/// ## Example
/// ```python
/// def plot(x, y, z, color, mark, add_trendline):
///     ...
///
///
/// plot(1, 2, 3, "r", "*", True)
/// ```
///
/// Use instead:
/// ```python
/// def plot(x, y, z, *, color, mark, add_trendline):
///     ...
///
///
/// plot(1, 2, 3, color="r", mark="*", add_trendline=True)
/// ```
///
/// ## Options
/// - `pylint.max-positional-args`
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

/// PLR0917
pub(crate) fn too_many_positional(checker: &mut Checker, function_def: &ast::StmtFunctionDef) {
    let num_positional_args = function_def
        .parameters
        .args
        .iter()
        .chain(&function_def.parameters.posonlyargs)
        .filter(|arg| {
            !checker
                .settings
                .dummy_variable_rgx
                .is_match(&arg.parameter.name)
        })
        .count();

    if num_positional_args > checker.settings.pylint.max_positional_args {
        // Allow excessive arguments in `@override` or `@overload` methods, since they're required
        // to adhere to the parent signature.
        if visibility::is_override(&function_def.decorator_list, checker.semantic())
            || visibility::is_overload(&function_def.decorator_list, checker.semantic())
        {
            return;
        }

        checker.diagnostics.push(Diagnostic::new(
            TooManyPositional {
                c_pos: num_positional_args,
                max_pos: checker.settings.pylint.max_positional_args,
            },
            function_def.identifier(),
        ));
    }
}
