use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};

use super::helpers;

/// ## What it does
/// Checks for `set()` calls that take unnecessary list or tuple literals
/// as arguments.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a call to `set()`.
/// Instead, the expression can be rewritten as a set literal.
///
/// ## Example
/// ```python
/// set([1, 2])
/// set((1, 2))
/// set([])
/// ```
///
/// Use instead:
/// ```python
/// {1, 2}
/// {1, 2}
/// set()
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryLiteralSet {
    kind: UnnecessaryLiteral,
}

impl AlwaysFixableViolation for UnnecessaryLiteralSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralSet { kind } = self;
        format!("Unnecessary {kind} literal (rewrite as a set literal)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a set literal".to_string()
    }
}

/// C405 (`set([1, 2])`)
pub(crate) fn unnecessary_literal_set(checker: &Checker, call: &ast::ExprCall) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function(
        "set",
        &call.func,
        &call.arguments.args,
        &call.arguments.keywords,
    ) else {
        return;
    };
    let Some(kind) = UnnecessaryLiteral::try_from_expr(argument) else {
        return;
    };
    if !checker.semantic().has_builtin_binding("set") {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnnecessaryLiteralSet { kind }, call.range());

    // Convert `set((1, 2))` to `{1, 2}`.
    diagnostic.set_fix({
        let elts = match &argument {
            Expr::List(ast::ExprList { elts, .. }) => elts,
            Expr::Tuple(ast::ExprTuple { elts, .. }) => elts,
            _ => unreachable!(),
        };

        match elts.as_slice() {
            // If the list or tuple is empty, replace the entire call with `set()`.
            [] => Fix::unsafe_edit(Edit::range_replacement("set()".to_string(), call.range())),
            // If it's a single-element tuple (with no whitespace around it), remove the trailing
            // comma.
            [elt]
                if argument.is_tuple_expr()
                    // The element must start right after the `(`.
                    && elt.start() == argument.start() + TextSize::new(1)
                    // The element must be followed by exactly one comma and a closing `)`.
                    && elt.end() + TextSize::new(2) == argument.end() =>
            {
                // Replace from the start of the call to the start of the inner element.
                let call_start = Edit::replacement(
                    pad_start("{", call.range(), checker.locator(), checker.semantic()),
                    call.start(),
                    elt.start(),
                );

                // Replace from the end of the inner element to the end of the call with `}`.
                let call_end = Edit::replacement(
                    pad_end("}", call.range(), checker.locator(), checker.semantic()),
                    elt.end(),
                    call.end(),
                );

                Fix::unsafe_edits(call_start, [call_end])
            }
            _ => {
                // Replace from the start of the call to the start of the inner list or tuple with `{`.
                let call_start = Edit::replacement(
                    pad_start("{", call.range(), checker.locator(), checker.semantic()),
                    call.start(),
                    argument.start() + TextSize::from(1),
                );

                // Replace from the end of the inner list or tuple to the end of the call with `}`.
                let call_end = Edit::replacement(
                    pad_end("}", call.range(), checker.locator(), checker.semantic()),
                    argument.end() - TextSize::from(1),
                    call.end(),
                );

                Fix::unsafe_edits(call_start, [call_end])
            }
        }
    });

    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnnecessaryLiteral {
    List,
    Tuple,
}

impl UnnecessaryLiteral {
    const fn try_from_expr(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::List(_) => Some(Self::List),
            Expr::Tuple(_) => Some(Self::Tuple),
            _ => None,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Tuple => "tuple",
            Self::List => "list",
        }
    }
}

impl std::fmt::Display for UnnecessaryLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
