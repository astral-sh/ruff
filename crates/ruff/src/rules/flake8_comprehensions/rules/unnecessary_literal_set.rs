use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for `set` calls that take unnecessary `list` or `tuple` literals
/// as arguments.
///
/// ## Why is it bad?
/// It's unnecessary to use a list or tuple literal within a call to `set`.
/// Instead, the expression can be rewritten as a set literal.
///
/// ## Examples
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
#[violation]
pub struct UnnecessaryLiteralSet {
    pub obj_type: String,
}

impl AlwaysAutofixableViolation for UnnecessaryLiteralSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralSet { obj_type } = self;
        format!("Unnecessary `{obj_type}` literal (rewrite as a `set` literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` literal".to_string()
    }
}

/// C405 (`set([1, 2])`)
pub fn unnecessary_literal_set(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function("set", func, args, keywords) else {
        return;
    };
    if !checker.ctx.is_builtin("set") {
        return;
    }
    let kind = match argument {
        ExprKind::List { .. } => "list",
        ExprKind::Tuple { .. } => "tuple",
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralSet {
            obj_type: kind.to_string(),
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_literal_set(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}
