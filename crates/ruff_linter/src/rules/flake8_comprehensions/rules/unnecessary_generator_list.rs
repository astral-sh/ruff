use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::ExprGenerator;
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as list
/// comprehensions (or with `list()` directly).
///
/// ## Why is this bad?
/// It is unnecessary to use `list()` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// Further, if the comprehension can be removed entirely, as in the case of
/// `list(x for x in foo)`, it's better to use `list(foo)` directly, since it's
/// even more direct.
///
/// ## Example
/// ```python
/// list(f(x) for x in foo)
/// list(x for x in foo)
/// list((x for x in foo))
/// ```
///
/// Use instead:
/// ```python
/// [f(x) for x in foo]
/// list(foo)
/// list(foo)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryGeneratorList {
    short_circuit: bool,
}

impl AlwaysFixableViolation for UnnecessaryGeneratorList {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.short_circuit {
            "Unnecessary generator (rewrite using `list()`)".to_string()
        } else {
            "Unnecessary generator (rewrite as a list comprehension)".to_string()
        }
    }

    fn fix_title(&self) -> String {
        if self.short_circuit {
            "Rewrite using `list()`".to_string()
        } else {
            "Rewrite as a list comprehension".to_string()
        }
    }
}

/// C400 (`list(generator)`)
pub(crate) fn unnecessary_generator_list(checker: &Checker, call: &ast::ExprCall) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function(
        "list",
        &call.func,
        &call.arguments.args,
        &call.arguments.keywords,
    ) else {
        return;
    };

    let ast::Expr::Generator(ExprGenerator {
        elt,
        generators,
        parenthesized,
        ..
    }) = argument
    else {
        return;
    };

    if !checker.semantic().has_builtin_binding("list") {
        return;
    }

    // Short-circuit: given `list(x for x in y)`, generate `list(y)` (in lieu of `[x for x in y]`).
    if let [generator] = generators.as_slice() {
        if generator.ifs.is_empty() && !generator.is_async {
            if ComparableExpr::from(elt) == ComparableExpr::from(&generator.target) {
                let diagnostic = Diagnostic::new(
                    UnnecessaryGeneratorList {
                        short_circuit: true,
                    },
                    call.range(),
                );
                let iterator = format!("list({})", checker.locator().slice(generator.iter.range()));
                let fix = Fix::unsafe_edit(Edit::range_replacement(iterator, call.range()));
                checker.report_diagnostic(diagnostic.with_fix(fix));
                return;
            }
        }
    }

    // Convert `list(f(x) for x in y)` to `[f(x) for x in y]`.
    let diagnostic = Diagnostic::new(
        UnnecessaryGeneratorList {
            short_circuit: false,
        },
        call.range(),
    );
    let fix = {
        // Replace `list(` with `[`.
        let call_start = Edit::replacement(
            "[".to_string(),
            call.start(),
            call.arguments.start() + TextSize::from(1),
        );

        // Replace `)` with `]`.
        // Place `]` at argument's end or at trailing comma if present
        let after_arg_tokens = checker
            .tokens()
            .in_range(TextRange::new(argument.end(), call.end()));
        let right_bracket_loc = after_arg_tokens
            .iter()
            .find(|token| token.kind() == TokenKind::Comma)
            .map_or(call.arguments.end(), Ranged::end)
            - TextSize::from(1);
        let call_end = Edit::replacement("]".to_string(), right_bracket_loc, call.end());

        // Remove the inner parentheses, if the expression is a generator. The easiest way to do
        // this reliably is to use the printer.
        if *parenthesized {
            // The generator's range will include the innermost parentheses, but it could be
            // surrounded by additional parentheses.
            let range = parenthesized_range(
                argument.into(),
                (&call.arguments).into(),
                checker.comment_ranges(),
                checker.locator().contents(),
            )
            .unwrap_or(argument.range());

            // The generator always parenthesizes the expression; trim the parentheses.
            let generator = checker.generator().expr(argument);
            let generator = generator[1..generator.len() - 1].to_string();

            let replacement = Edit::range_replacement(generator, range);
            Fix::unsafe_edits(call_start, [call_end, replacement])
        } else {
            Fix::unsafe_edits(call_start, [call_end])
        }
    };
    checker.report_diagnostic(diagnostic.with_fix(fix));
}
