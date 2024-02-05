use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for `tuple` calls that take unnecessary list or tuple literals as
/// arguments.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a `tuple()` call,
/// since there is a literal syntax for these types.
///
/// If a list literal was passed, then it should be rewritten as a `tuple`
/// literal. Otherwise, if a tuple literal was passed, then the outer call
/// to `list()` should be removed.
///
/// ## Examples
/// ```python
/// tuple([1, 2])
/// tuple((1, 2))
/// ```
///
/// Use instead:
/// ```python
/// (1, 2)
/// (1, 2)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[violation]
pub struct UnnecessaryLiteralWithinTupleCall {
    literal: String,
}

impl AlwaysFixableViolation for UnnecessaryLiteralWithinTupleCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall { literal } = self;
        if literal == "list" {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (rewrite as a `tuple` literal)"
            )
        } else {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (remove the outer call to `tuple()`)"
            )
        }
    }

    fn fix_title(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall { literal } = self;
        {
            if literal == "list" {
                "Rewrite as a `tuple` literal".to_string()
            } else {
                "Remove outer `tuple` call".to_string()
            }
        }
    }
}

/// C409
pub(crate) fn unnecessary_literal_within_tuple_call(checker: &mut Checker, call: &ast::ExprCall) {
    if !call.arguments.keywords.is_empty() {
        return;
    }
    let Some(argument) =
        helpers::first_argument_with_matching_function("tuple", &call.func, &call.arguments.args)
    else {
        return;
    };
    if !checker.semantic().is_builtin("tuple") {
        return;
    }
    let argument_kind = match argument {
        Expr::Tuple(_) => "tuple",
        Expr::List(_) => "list",
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinTupleCall {
            literal: argument_kind.to_string(),
        },
        call.range(),
    );

    // Convert `tuple([1, 2])` to `tuple(1, 2)`
    diagnostic.set_fix({
        // Replace from the start of the call to the start of the inner list or tuple with `(`.
        let call_start = Edit::replacement(
            "(".to_string(),
            call.start(),
            argument.start() + TextSize::from(1),
        );

        // Replace from the end of the inner list or tuple to the end of the call with `)`.
        let call_end = Edit::replacement(
            ")".to_string(),
            argument.end() - TextSize::from(1),
            call.end(),
        );

        Fix::unsafe_edits(call_start, [call_end])
    });

    checker.diagnostics.push(diagnostic);
}
