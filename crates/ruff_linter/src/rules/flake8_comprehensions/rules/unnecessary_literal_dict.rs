use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprCall, ExprKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_comprehensions::fixes;
use crate::rules::flake8_comprehensions::helpers::exactly_one_argument_with_matching_function;
use crate::{AlwaysFixableViolation, Fix};

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
#[violation_metadata(stable_since = "v0.0.61", category = Category::Complexity)]
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
pub(crate) fn unnecessary_literal_dict(checker: &Checker, call: ExprCall<'_>) {
    let Some(argument) = exactly_one_argument_with_matching_function("dict", call) else {
        return;
    };
    let (kind, elts) = match argument.classify() {
        ExprKind::Tuple(expr_tuple) => {
            let elts = expr_tuple.elts();

            (LiteralKind::Tuple, elts)
        }
        ExprKind::List(expr_list) => {
            let elts = expr_list.elts();
            (LiteralKind::List, elts)
        }
        _ => return,
    };
    // Accept `dict((1, 2), ...))` `dict([(1, 2), ...])`.
    if !elts
        .iter()
        .all(|elt| matches!(elt.classify(), ExprKind::Tuple(tuple) if tuple.elts().len() == 2))
    {
        return;
    }
    if !checker.semantic().has_builtin_binding("dict") {
        return;
    }
    let mut diagnostic =
        checker.report_diagnostic(UnnecessaryLiteralDict { obj_type: kind }, call.range());
    diagnostic.try_set_fix(|| {
        fixes::fix_unnecessary_literal_dict(call.into(), checker).map(Fix::unsafe_edit)
    });
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
