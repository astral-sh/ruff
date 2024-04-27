use std::fmt;

use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::helpers;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DictKind {
    Literal,
    Comprehension,
}

impl fmt::Display for DictKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DictKind::Literal => fmt.write_str("literal"),
            DictKind::Comprehension => fmt.write_str("comprehension"),
        }
    }
}

/// ## What it does
/// Checks for `dict` calls that take unnecessary `dict` literals or `dict`
/// comprehensions as arguments.
///
/// ## Why is this bad?
/// It's unnecessary to wrap a `dict` literal or comprehension within a `dict`
/// call, since the literal or comprehension syntax already returns a `dict`.
///
/// ## Examples
/// ```python
/// dict({})
/// dict({"a": 1})
/// ```
///
/// Use instead:
/// ```python
/// {}
/// {"a": 1}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[violation]
pub struct UnnecessaryLiteralWithinDictCall {
    kind: DictKind,
}

impl AlwaysFixableViolation for UnnecessaryLiteralWithinDictCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinDictCall { kind } = self;
        format!("Unnecessary `dict` {kind} passed to `dict()` (remove the outer call to `dict()`)")
    }

    fn fix_title(&self) -> String {
        "Remove outer `dict` call".to_string()
    }
}

/// C418
pub(crate) fn unnecessary_literal_within_dict_call(checker: &mut Checker, call: &ast::ExprCall) {
    if !call.arguments.keywords.is_empty() {
        return;
    }
    let Some(argument) =
        helpers::first_argument_with_matching_function("dict", &call.func, &call.arguments.args)
    else {
        return;
    };
    if !checker.semantic().has_builtin_binding("dict") {
        return;
    }
    let argument_kind = match argument {
        Expr::DictComp(_) => DictKind::Comprehension,
        Expr::Dict(_) => DictKind::Literal,
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinDictCall {
            kind: argument_kind,
        },
        call.range(),
    );

    // Convert `dict({"a": 1})` to `{"a": 1}`
    diagnostic.set_fix({
        // Delete from the start of the call to the start of the argument.
        let call_start = Edit::deletion(call.start(), argument.start());

        // Delete from the end of the argument to the end of the call.
        let call_end = Edit::deletion(argument.end(), call.end());

        Fix::unsafe_edits(call_start, [call_end])
    });

    checker.diagnostics.push(diagnostic);
}
