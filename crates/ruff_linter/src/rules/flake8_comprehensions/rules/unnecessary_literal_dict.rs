use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary list or tuple literals.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a call to `dict()`.
/// It can be rewritten as a dict literal (`{}`).
///
/// ## Example
/// ```python
/// dict([(1, 2), (3, 4)])
/// dict(((1, 2), (3, 4)))
/// dict([])
/// ```
///
/// Use instead:
/// ```python
/// {1: 2, 3: 4}
/// {1: 2, 3: 4}
/// {}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryLiteralDict {
    obj_type: LiteralKind,
}

impl AlwaysFixableViolation for UnnecessaryLiteralDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralDict { obj_type } = self;
        format!("Unnecessary {obj_type} literal (rewrite as a dict literal)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a dict literal".to_string()
    }
}

/// C406 (`dict([(1, 2)])`)
pub(crate) fn unnecessary_literal_dict(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) =
        helpers::exactly_one_argument_with_matching_function("dict", func, args, keywords)
    else {
        return;
    };
    let (kind, elts) = match argument {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => (LiteralKind::Tuple, elts),
        Expr::List(ast::ExprList { elts, .. }) => (LiteralKind::List, elts),
        _ => return,
    };
    // Accept `dict((1, 2), ...))` `dict([(1, 2), ...])`.
    if !elts
        .iter()
        .all(|elt| matches!(&elt, Expr::Tuple(tuple) if tuple.len() == 2))
    {
        return;
    }
    if !checker.semantic().has_builtin_binding("dict") {
        return;
    }
    let mut diagnostic = Diagnostic::new(UnnecessaryLiteralDict { obj_type: kind }, expr.range());
    diagnostic
        .try_set_fix(|| fixes::fix_unnecessary_literal_dict(expr, checker).map(Fix::unsafe_edit));
    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum LiteralKind {
    Tuple,
    List,
}

impl LiteralKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Tuple => "tuple",
            Self::List => "list",
        }
    }
}

impl std::fmt::Display for LiteralKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
