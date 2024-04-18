use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for any usage of `__cached__` and `__file__` as an argument to
/// `logging.getLogger()`.
///
/// ## Why is this bad?
/// The [logging documentation] recommends this pattern:
///
/// ```python
/// logging.getLogger(__name__)
/// ```
///
/// Here, `__name__` is the fully qualified module name, such as `foo.bar`,
/// which is the intended format for logger names.
///
/// This rule detects probably-mistaken usage of similar module-level dunder constants:
///
/// * `__cached__` - the pathname of the module's compiled version, such as `foo/__pycache__/bar.cpython-311.pyc`.
/// * `__file__` - the pathname of the module, such as `foo/bar.py`.
///
/// ## Example
/// ```python
/// import logging
///
/// logger = logging.getLogger(__file__)
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// logger = logging.getLogger(__name__)
/// ```
///
/// [logging documentation]: https://docs.python.org/3/library/logging.html#logger-objects
#[violation]
pub struct InvalidGetLoggerArgument;

impl Violation for InvalidGetLoggerArgument {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `__name__` with `logging.getLogger()`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `__name__`"))
    }
}

/// LOG002
pub(crate) fn invalid_get_logger_argument(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::LOGGING) {
        return;
    }

    let Some(Expr::Name(expr @ ast::ExprName { id, .. })) = call.arguments.find_argument("name", 0)
    else {
        return;
    };

    if !matches!(id.as_ref(), "__file__" | "__cached__") {
        return;
    }

    if !checker.semantic().has_builtin_binding(id) {
        return;
    }

    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["logging", "getLogger"]))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(InvalidGetLoggerArgument, expr.range());
    if checker.semantic().has_builtin_binding("__name__") {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            "__name__".to_string(),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
