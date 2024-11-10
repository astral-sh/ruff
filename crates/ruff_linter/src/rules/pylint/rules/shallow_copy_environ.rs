use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for shallow `os.environ` copies.
///
/// ## Why is this bad?
/// `os.environ` is not a dict object but proxy object, so shallow copy has still
/// effects on original object. See https://bugs.python.org/issue15373 for reference.
///
/// ## Example
/// ```python
/// import copy
/// import os
///
/// copied_env = copy.copy(os.environ)
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// copied_env = os.environ.copy()
/// ```
#[violation]
pub struct ShallowCopyEnviron;

impl AlwaysFixableViolation for ShallowCopyEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `copy.copy(os.environ)`. Use `os.environ.copy()` instead.".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `os.environ.copy()`".to_string()
    }
}

/// PLW1507
pub(crate) fn shallow_copy_environ(checker: &mut Checker, call: &ast::ExprCall) {
    if !(checker.semantic().seen_module(Modules::OS)
        && checker.semantic().seen_module(Modules::COPY))
    {
        return;
    }

    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["copy", "copy"]))
    {
        return;
    }

    let Some(first_arg) = call.arguments.args.first() else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(first_arg)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "environ"]))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(ShallowCopyEnviron, call.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "os.environ.copy()".to_string(),
        call.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
