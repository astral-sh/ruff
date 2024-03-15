use ast::{comparable::ComparableExpr, ExprGenerator};
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `list`
/// comprehensions or conversion to `list` using the builtin function.
///
/// ## Why is this bad?
/// It is unnecessary to use `list` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension/conversion to `list` is clearer and more idiomatic.
///
/// ## Examples
/// ```python
/// list(f(x) for x in foo)
/// list(x for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// [f(x) for x in foo]
/// list(foo)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[violation]
pub struct UnnecessaryGeneratorList {
    shortened_to_list_conversion: bool,
}

impl AlwaysFixableViolation for UnnecessaryGeneratorList {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.shortened_to_list_conversion {
            format!("Unnecessary generator (rewrite using `list()`")
        } else {
            format!("Unnecessary generator (rewrite as a `list` comprehension)")
        }
    }

    fn fix_title(&self) -> String {
        if self.shortened_to_list_conversion {
            "Rewrite using `list()`".to_string()
        } else {
            "Rewrite as a `list` comprehension".to_string()
        }
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
    if let Some(generator_expr) = argument.clone().generator_expr() {
        // Short-circuit the case for list(x for x in y) -> list(y)
        let mut shortcircuit_completed = false;
        'shortcircuit: {
            let mut diagnostic = Diagnostic::new(
                UnnecessaryGeneratorList {
                    shortened_to_list_conversion: true,
                },
                call.range(),
            );

            let ExprGenerator {
                range: _,
                elt,
                generators,
                parenthesized: _,
            } = generator_expr;

            let [generator] = &generators[..] else {
                break 'shortcircuit;
            };
            if !generator.ifs.is_empty() || generator.is_async {
                break 'shortcircuit;
            };
            if ComparableExpr::from(&elt) != ComparableExpr::from(&generator.target) {
                break 'shortcircuit;
            };

            let iterator = format!("list({})", checker.locator().slice(generator.iter.range()));
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                iterator,
                call.range(),
            )));
            checker.diagnostics.push(diagnostic);
            shortcircuit_completed = true;
        }

        // Convert `list(f(x) for x in y)` to `[f(x) for x in y]`.
        if !shortcircuit_completed {
            let mut diagnostic = Diagnostic::new(
                UnnecessaryGeneratorList {
                    shortened_to_list_conversion: false,
                },
                call.range(),
            );
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
}
