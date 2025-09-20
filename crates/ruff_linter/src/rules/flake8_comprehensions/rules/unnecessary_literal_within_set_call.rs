use std::fmt;

use ruff_python_ast::{self as ast, Expr};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

use crate::rules::flake8_comprehensions::helpers;

/// ## What it does
/// Checks for `set()` calls that take unnecessary set literals or set
/// comprehensions as arguments.
///
/// ## Why is this bad?
/// It's unnecessary to wrap a set literal or comprehension within a `set()`
/// call, since the literal or comprehension syntax already returns a
/// set.
///
/// ## Example
/// ```python
/// set({1, 2, 3})
/// set({x for x in range(10)})
/// ```
///
/// Use instead:
/// ```python
/// {1, 2, 3}
/// {x for x in range(10)}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryLiteralWithinSetCall {
    kind: SetKind,
}

impl AlwaysFixableViolation for UnnecessaryLiteralWithinSetCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinSetCall { kind } = self;
        format!("Unnecessary set {kind} passed to `set()` (remove the outer call to `set()`)")
    }

    fn fix_title(&self) -> String {
        "Remove outer `set()` call".to_string()
    }
}

/// C421
pub(crate) fn unnecessary_literal_within_set_call(checker: &Checker, call: &ast::ExprCall) {
    if !call.arguments.keywords.is_empty() {
        return;
    }
    if call.arguments.args.len() > 1 {
        return;
    }
    let Some(argument) =
        helpers::first_argument_with_matching_function("set", &call.func, &call.arguments.args)
    else {
        return;
    };
    let Some(argument_kind) = SetKind::try_from_expr(argument) else {
        return;
    };
    if !checker.semantic().has_builtin_binding("set") {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryLiteralWithinSetCall {
            kind: argument_kind,
        },
        call.range(),
    );

    // Convert `set({1, 2, 3})` to `{1, 2, 3}`
    diagnostic.set_fix({
        // Delete from the start of the call to the start of the argument.
        let call_start = Edit::deletion(call.start(), argument.start());

        // Delete from the end of the argument to the end of the call.
        let call_end = Edit::deletion(argument.end(), call.end());

        Fix::unsafe_edits(call_start, [call_end])
    });
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum SetKind {
    Literal,
    Comprehension,
}

impl SetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Literal => "literal",
            Self::Comprehension => "comprehension",
        }
    }

    const fn try_from_expr(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::Set(_) => Some(Self::Literal),
            Expr::SetComp(_) => Some(Self::Comprehension),
            _ => None,
        }
    }
}

impl fmt::Display for SetKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
