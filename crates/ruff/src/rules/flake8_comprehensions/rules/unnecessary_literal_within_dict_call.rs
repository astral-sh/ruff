use rustpython_parser::ast::{Expr, ExprKind, Keyword};
use std::fmt;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

#[derive(Debug, PartialEq, Eq)]
pub enum DictKind {
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
/// ## Why is it bad?
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
    pub kind: DictKind,
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
pub fn unnecessary_literal_within_dict_call(
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
    if !checker.ctx.is_builtin("dict") {
        return;
    }
    let argument_kind = match argument {
        ExprKind::DictComp { .. } => DictKind::Comprehension,
        ExprKind::Dict { .. } => DictKind::Literal,
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinDictCall {
            kind: argument_kind,
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_literal_within_dict_call(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}
