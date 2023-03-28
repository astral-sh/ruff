use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;
use crate::rules::flake8_comprehensions::settings::Settings;

use super::helpers;

/// ## What it does
/// Checks for unnecessary `dict`, `list` or `tuple` calls that can be
/// rewritten as empty literals.
///
/// ## Why is this bad?
/// It's unnecessary to call e.g., `dict()` as opposed to using an empty
/// literal (`{}`). The former is slower because the name `dict` must be
/// looked up in the global scope in case it has been rebound.
///
/// ## Examples
/// ```python
/// dict()
/// dict(a=1, b=2)
/// list()
/// tuple()
/// ```
///
/// Use instead:
/// ```python
/// {}
/// {"a": 1, "b": 2}
/// []
/// ()
/// ```
#[violation]
pub struct UnnecessaryCollectionCall {
    pub obj_type: String,
}

impl AlwaysAutofixableViolation for UnnecessaryCollectionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCollectionCall { obj_type } = self;
        format!("Unnecessary `{obj_type}` call (rewrite as a literal)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a literal".to_string()
    }
}

/// C408
pub fn unnecessary_collection_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    settings: &Settings,
) {
    if !args.is_empty() {
        return;
    }
    let Some(id) = helpers::expr_name(func) else {
        return;
    };
    match id {
        "dict"
            if keywords.is_empty()
                || (!settings.allow_dict_calls_with_keyword_arguments
                    && keywords.iter().all(|kw| kw.node.arg.is_some())) =>
        {
            // `dict()` or `dict(a=1)` (as opposed to `dict(**a)`)
        }
        "list" | "tuple" => {
            // `list()` or `tuple()`
        }
        _ => return,
    };
    if !checker.ctx.is_builtin(id) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryCollectionCall {
            obj_type: id.to_string(),
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_collection_call(checker.locator, checker.stylist, expr)
        });
    }
    checker.diagnostics.push(diagnostic);
}
