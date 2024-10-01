use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::ExprGenerator;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `set`
/// comprehensions (or with `set` directly).
///
/// ## Why is this bad?
/// It is unnecessary to use `set` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// Further, if the comprehension can be removed entirely, as in the case of
/// `set(x for x in foo)`, it's better to use `set(foo)` directly, since it's
/// even more direct.
///
/// ## Examples
/// ```python
/// set(f(x) for x in foo)
/// set(x for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// {f(x) for x in foo}
/// set(foo)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[violation]
pub struct UnnecessaryGeneratorSet {
    short_circuit: bool,
}

impl AlwaysFixableViolation for UnnecessaryGeneratorSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.short_circuit {
            format!("Unnecessary generator (rewrite using `set()`)")
        } else {
            format!("Unnecessary generator (rewrite as a `set` comprehension)")
        }
    }

    fn fix_title(&self) -> String {
        if self.short_circuit {
            "Rewrite using `set()`".to_string()
        } else {
            "Rewrite as a `set` comprehension".to_string()
        }
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
    if !checker.semantic().has_builtin_binding("set") {
        return;
    }

    let Some(ExprGenerator {
        elt, generators, ..
    }) = argument.as_generator_expr()
    else {
        return;
    };

    // Short-circuit: given `set(x for x in y)`, generate `set(y)` (in lieu of `{x for x in y}`).
    if let [generator] = generators.as_slice() {
        if generator.ifs.is_empty() && !generator.is_async {
            if ComparableExpr::from(elt) == ComparableExpr::from(&generator.target) {
                let mut diagnostic = Diagnostic::new(
                    UnnecessaryGeneratorSet {
                        short_circuit: true,
                    },
                    call.range(),
                );
                let iterator = format!("set({})", checker.locator().slice(generator.iter.range()));
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    iterator,
                    call.range(),
                )));
                checker.diagnostics.push(diagnostic);
                return;
            }
        }
    }

    // Convert `set(f(x) for x in y)` to `{f(x) for x in y}`.
    let mut diagnostic = Diagnostic::new(
        UnnecessaryGeneratorSet {
            short_circuit: false,
        },
        call.range(),
    );
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
