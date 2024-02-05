use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `set`
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `set` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Examples
/// ```python
/// set(f(x) for x in foo)
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
pub struct UnnecessaryGeneratorSet;

impl AlwaysFixableViolation for UnnecessaryGeneratorSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `set` comprehension)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }
}

/// C401 (`set(generator)`)
pub(crate) fn unnecessary_generator_set(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function(
        "set",
        &call.func,
        &call.arguments.args,
        &call.arguments.keywords,
    ) else {
        return;
    };
    if !checker.semantic().is_builtin("set") {
        return;
    }
    if argument.is_generator_exp_expr() {
        let mut diagnostic = Diagnostic::new(UnnecessaryGeneratorSet, call.range());

        // Convert `set(x for x in y)` to `{x for x in y}`.
        diagnostic.set_fix({
            // Replace `set(` with `}`.
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

            Fix::unsafe_edits(call_start, [call_end])
        });

        checker.diagnostics.push(diagnostic);
    }
}
