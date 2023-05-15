use rustpython_parser::ast::{self, Expr, ExprKind, Keyword};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};

use super::helpers;

/// ## What it does
/// Checks for unnecessary `list` or `tuple` literals.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a call to `dict`.
/// It can be rewritten as a dict literal (`{}`).
///
/// ## Examples
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
#[violation]
pub struct UnnecessaryLiteralDict {
    obj_type: String,
}

impl AlwaysAutofixableViolation for UnnecessaryLiteralDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralDict { obj_type } = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `dict` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` literal".to_string()
    }
}

/// C406 (`dict([(1, 2)])`)
pub(crate) fn unnecessary_literal_dict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function("dict", func, args, keywords) else {
        return;
    };
    if !checker.ctx.is_builtin("dict") {
        return;
    }
    let (kind, elts) = match argument {
        ExprKind::Tuple(ast::ExprTuple { elts, .. }) => ("tuple", elts),
        ExprKind::List(ast::ExprList { elts, .. }) => ("list", elts),
        _ => return,
    };
    // Accept `dict((1, 2), ...))` `dict([(1, 2), ...])`.
    if !elts.iter().all(
        |elt| matches!(&elt.node, ExprKind::Tuple(ast::ExprTuple { elts, .. } )if elts.len() == 2),
    ) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralDict {
            obj_type: kind.to_string(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.try_set_fix_from_edit(|| {
            fixes::fix_unnecessary_literal_dict(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}
