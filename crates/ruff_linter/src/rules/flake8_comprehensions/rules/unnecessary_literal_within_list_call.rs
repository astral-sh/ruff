use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for `list()` calls that take unnecessary list or tuple literals as
/// arguments.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a `list()` call,
/// since there is a literal syntax for these types.
///
/// If a list literal is passed in, then the outer call to `list()` should be
/// removed. Otherwise, if a tuple literal is passed in, then it should be
/// rewritten as a list literal.
///
/// ## Example
/// ```python
/// list([1, 2])
/// list((1, 2))
/// ```
///
/// Use instead:
/// ```python
/// [1, 2]
/// [1, 2]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryLiteralWithinListCall {
    kind: LiteralKind,
}

impl AlwaysFixableViolation for UnnecessaryLiteralWithinListCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            LiteralKind::List => {
                "Unnecessary list literal passed to `list()` (remove the outer call to `list()`)"
                    .to_string()
            }
            LiteralKind::Tuple => {
                "Unnecessary tuple literal passed to `list()` (rewrite as a single list literal)"
                    .to_string()
            }
        }
    }

    fn fix_title(&self) -> String {
        match self.kind {
            LiteralKind::List => "Remove outer `list()` call".to_string(),
            LiteralKind::Tuple => "Rewrite as a single list literal".to_string(),
        }
    }
}

/// C410
pub(crate) fn unnecessary_literal_within_list_call(checker: &Checker, call: &ast::ExprCall) {
    if !call.arguments.keywords.is_empty() {
        return;
    }
    if call.arguments.args.len() > 1 {
        return;
    }
    let Some(argument) =
        helpers::first_argument_with_matching_function("list", &call.func, &call.arguments.args)
    else {
        return;
    };
    let Some(argument_kind) = LiteralKind::try_from_expr(argument) else {
        return;
    };
    if !checker.semantic().has_builtin_binding("list") {
        return;
    }

    let diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinListCall {
            kind: argument_kind,
        },
        call.range(),
    );

    // Convert `list([1, 2])` to `[1, 2]`
    let fix = {
        // Delete from the start of the call to the start of the argument.
        let call_start = Edit::deletion(call.start(), argument.start());

        // Delete from the end of the argument to the end of the call.
        let call_end = Edit::deletion(argument.end(), call.end());

        // If this is a tuple, we also need to convert the inner argument to a list.
        if argument.is_tuple_expr() {
            // Replace `(` with `[`.
            let argument_start = Edit::replacement(
                "[".to_string(),
                argument.start(),
                argument.start() + TextSize::from(1),
            );

            // Replace `)` with `]`.
            let argument_end = Edit::replacement(
                "]".to_string(),
                argument.end() - TextSize::from(1),
                argument.end(),
            );

            Fix::unsafe_edits(call_start, [argument_start, argument_end, call_end])
        } else {
            Fix::unsafe_edits(call_start, [call_end])
        }
    };

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum LiteralKind {
    Tuple,
    List,
}

impl LiteralKind {
    const fn try_from_expr(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::Tuple(_) => Some(Self::Tuple),
            Expr::List(_) => Some(Self::List),
            _ => None,
        }
    }
}
