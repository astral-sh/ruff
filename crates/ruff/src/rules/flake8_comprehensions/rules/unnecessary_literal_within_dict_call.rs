use std::fmt;

use ruff_python_ast::{Expr, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

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
#[violation]
pub struct UnnecessaryLiteralWithinDictCall {
    kind: DictKind,
}

impl AlwaysAutofixableViolation for UnnecessaryLiteralWithinDictCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinDictCall { kind } = self;
        format!("Unnecessary `dict` {kind} passed to `dict()` (remove the outer call to `dict()`)")
    }

    fn autofix_title(&self) -> String {
        "Remove outer `dict` call".to_string()
    }
}

/// C418
pub(crate) fn unnecessary_literal_within_dict_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !keywords.is_empty() {
        return;
    }
    let Some(argument) = helpers::first_argument_with_matching_function("dict", func, args) else {
        return;
    };
    if !checker.semantic().is_builtin("dict") {
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
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_literal_within_dict_call(
                expr,
                checker.locator(),
                checker.stylist(),
            )
            .map(Fix::suggested)
        });
    }
    checker.diagnostics.push(diagnostic);
}
