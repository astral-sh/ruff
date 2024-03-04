use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::{self as ast};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `subprocess.Popen` with a `preexec_fn` argument.
///
/// ## Why is this bad?
/// The `preexec_fn` argument is unsafe within threads as it can lead to
/// deadlocks. Furthermore, `preexec_fn` is [targeted for deprecation].
///
/// Instead, consider using task-specific arguments such as `env`,
/// `start_new_session`, and `process_group`. These are not prone to deadlocks
/// and are more explicit.
///
/// ## Example
/// ```python
/// import os, subprocess
///
/// subprocess.Popen(foo, preexec_fn=os.setsid)
/// subprocess.Popen(bar, preexec_fn=os.setpgid(0, 0))
/// ```
///
/// Use instead:
/// ```python
/// import subprocess
///
/// subprocess.Popen(foo, start_new_session=True)
/// subprocess.Popen(bar, process_group=0)  # Introduced in Python 3.11
/// ```
///
/// ## References
/// - [Python documentation: `subprocess.Popen`](https://docs.python.org/3/library/subprocess.html#popen-constructor)
/// - [Why `preexec_fn` in `subprocess.Popen` may lead to deadlock?](https://discuss.python.org/t/why-preexec-fn-in-subprocess-popen-may-lead-to-deadlock/16908/2)
///
/// [targeted for deprecation]: https://github.com/python/cpython/issues/82616
#[violation]
pub struct SubprocessPopenPreexecFn;

impl Violation for SubprocessPopenPreexecFn {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`preexec_fn` argument is unsafe when using threads")
    }
}

/// PLW1509
pub(crate) fn subprocess_popen_preexec_fn(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::SUBPROCESS) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["subprocess", "Popen"]))
    {
        if let Some(keyword) = call
            .arguments
            .find_keyword("preexec_fn")
            .filter(|keyword| !keyword.value.is_none_literal_expr())
        {
            checker
                .diagnostics
                .push(Diagnostic::new(SubprocessPopenPreexecFn, keyword.range()));
        }
    }
}
