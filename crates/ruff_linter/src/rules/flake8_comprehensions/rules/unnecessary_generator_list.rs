use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `list`
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `list` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Examples
/// ```python
/// list(f(x) for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// [f(x) for x in foo]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[violation]
pub struct UnnecessaryGeneratorList;

impl AlwaysFixableViolation for UnnecessaryGeneratorList {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `list` comprehension)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a `list` comprehension".to_string()
    }
}

/// C400 (`list(generator)`)
pub(crate) fn unnecessary_generator_list(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function(
        "list",
        &call.func,
        &call.arguments.args,
        &call.arguments.keywords,
    ) else {
        return;
    };
    if !checker.semantic().is_builtin("list") {
        return;
    }
    if argument.is_generator_exp_expr() {
        let mut diagnostic = Diagnostic::new(UnnecessaryGeneratorList, call.range());

        // Convert `list(x for x in y)` to `[x for x in y]`.
        diagnostic.set_fix({
            // Replace `list(` with `[`.
            let call_start = Edit::replacement(
                "[".to_string(),
                call.start(),
                call.arguments.start() + TextSize::from(1),
            );

            // Replace `)` with `]`.
            let call_end = Edit::replacement(
                "]".to_string(),
                call.arguments.end() - TextSize::from(1),
                call.end(),
            );

            Fix::unsafe_edits(call_start, [call_end])
        });

        checker.diagnostics.push(diagnostic);
    }
}
