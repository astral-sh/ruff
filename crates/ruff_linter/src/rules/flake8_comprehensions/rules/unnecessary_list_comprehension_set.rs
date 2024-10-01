use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};

use super::helpers;

/// ## What it does
/// Checks for unnecessary list comprehensions.
///
/// ## Why is this bad?
/// It's unnecessary to use a list comprehension inside a call to `set`,
/// since there is an equivalent comprehension for this type.
///
/// ## Examples
/// ```python
/// set([f(x) for x in foo])
/// ```
///
/// Use instead:
/// ```python
/// {f(x) for x in foo}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[violation]
pub struct UnnecessaryListComprehensionSet;

impl AlwaysFixableViolation for UnnecessaryListComprehensionSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` comprehension (rewrite as a `set` comprehension)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }
}

/// C403 (`set([...])`)
pub(crate) fn unnecessary_list_comprehension_set(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function(
        "set",
        &call.func,
        &call.arguments.args,
        &call.arguments.keywords,
    ) else {
        return;
    };
    if !checker.semantic().has_builtin_binding("set") {
        return;
    }
    if argument.is_list_comp_expr() {
        let mut diagnostic = Diagnostic::new(UnnecessaryListComprehensionSet, call.range());
        diagnostic.set_fix({
            // Replace `set(` with `{`.
            let call_start = Edit::replacement(
                pad_start("{", call.range(), checker.locator(), checker.semantic()),
                call.start(),
                call.arguments.start() + TextSize::from(1),
            );

            // Replace `)` with `}`.
            let call_end = Edit::replacement(
                pad_end("}", call.range(), checker.locator(), checker.semantic()),
                call.arguments.end() - TextSize::from(1),
                call.end(),
            );

            // Delete the open bracket (`[`).
            let argument_start =
                Edit::deletion(argument.start(), argument.start() + TextSize::from(1));

            // Delete the close bracket (`]`).
            let argument_end = Edit::deletion(argument.end() - TextSize::from(1), argument.end());

            Fix::unsafe_edits(call_start, [argument_start, argument_end, call_end])
        });
        checker.diagnostics.push(diagnostic);
    }
}
