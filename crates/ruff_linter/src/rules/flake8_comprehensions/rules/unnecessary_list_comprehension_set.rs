use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};

use super::helpers;

/// ## What it does
/// Checks for unnecessary list comprehensions.
///
/// ## Why is this bad?
/// It's unnecessary to use a list comprehension inside a call to `set()`,
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
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryListComprehensionSet;

impl AlwaysFixableViolation for UnnecessaryListComprehensionSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary list comprehension (rewrite as a set comprehension)".to_string()
    }

    fn fix_title(&self) -> String {
        "Rewrite as a set comprehension".to_string()
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
    if !argument.is_list_comp_expr() {
        return;
    }
    let diagnostic = Diagnostic::new(UnnecessaryListComprehensionSet, call.range());
    let one = TextSize::from(1);

    // Replace `set(` with `{`.
    let call_start = Edit::replacement(
        pad_start("{", call.range(), checker.locator(), checker.semantic()),
        call.start(),
        call.arguments.start() + one,
    );

    // Replace `)` with `}`.
    let call_end = Edit::replacement(
        pad_end("}", call.range(), checker.locator(), checker.semantic()),
        call.arguments.end() - one,
        call.end(),
    );

    // If the list comprehension is parenthesized, remove the parentheses in addition to
    // removing the brackets.
    let replacement_range = parenthesized_range(
        argument.into(),
        (&call.arguments).into(),
        checker.comment_ranges(),
        checker.locator().contents(),
    )
    .unwrap_or_else(|| argument.range());

    let span = argument.range().add_start(one).sub_end(one);
    let replacement =
        Edit::range_replacement(checker.source()[span].to_string(), replacement_range);
    let fix = Fix::unsafe_edits(call_start, [call_end, replacement]);
    checker.report_diagnostic(diagnostic.with_fix(fix));
}
