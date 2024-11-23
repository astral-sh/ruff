use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for shallow `os.environ` copies.
///
/// ## Why is this bad?
/// `os.environ` is not a `dict` object, but rather, a proxy object. As such, mutating a shallow
/// copy of `os.environ` will also mutate the original object.
///
/// See: [#15373] for more information.
///
/// ## Example
/// ```python
/// import copy
/// import os
///
/// env = copy.copy(os.environ)
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// env = os.environ.copy()
/// ```
///
/// ## References
/// - [Python documentation: `copy` — Shallow and deep copy operations](https://docs.python.org/3/library/copy.html)
/// - [Python documentation: `os.environ`](https://docs.python.org/3/library/os.html#os.environ)
///
/// [#15373]: https://bugs.python.org/issue15373
#[derive(ViolationMetadata)]
pub(crate) struct ShallowCopyEnviron;

impl AlwaysFixableViolation for ShallowCopyEnviron {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Shallow copy of `os.environ` via `copy.copy(os.environ)`".to_string()
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

    if !call.arguments.keywords.is_empty() {
        return;
    }

    let [arg] = call.arguments.args.as_ref() else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(arg)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["os", "environ"]))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(ShallowCopyEnviron, call.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!("{}.copy()", checker.locator().slice(arg)),
        call.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
