use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;
use crate::rules::flake8_comprehensions::settings::Settings;

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
    obj_type: String,
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
pub(crate) fn unnecessary_collection_call(
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
    let Some(func) = func.as_name_expr() else {
        return;
    };
    match func.id.as_str() {
        "dict"
            if keywords.is_empty()
                || (!settings.allow_dict_calls_with_keyword_arguments
                    && keywords.iter().all(|kw| kw.arg.is_some())) =>
        {
            // `dict()` or `dict(a=1)` (as opposed to `dict(**a)`)
        }
        "list" | "tuple" => {
            // `list()` or `tuple()`
        }
        _ => return,
    };
    if !checker.semantic().is_builtin(func.id.as_str()) {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryCollectionCall {
            obj_type: func.id.to_string(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_collection_call(expr, checker).map(Fix::suggested)
        });
    }
    checker.diagnostics.push(diagnostic);
}
