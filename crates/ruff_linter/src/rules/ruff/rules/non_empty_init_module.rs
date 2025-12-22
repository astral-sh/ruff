use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::{Violation, checkers::ast::Checker};

/// ## What it does
///
/// Detects the presence of code in `__init__.py` files.
///
/// ## Why is this bad?
///
/// Including code in `__init__.py` files can cause surprising slowdowns because the code is run at
/// import time. `__init__.py` files are also often empty, so it's easy to overlook them when
/// debugging these issues.
///
/// ## Example
///
/// ```python
/// """My module docstring."""
///
///
/// class MyClass:
///     def my_method(self): ...
/// ```
///
/// Use instead:
///
/// ```python
/// """My module docstring."""
///
/// from submodule import MyClass
///
/// __all__ = ["MyClass"]
/// ```
///
/// When [`lint.ruff.strictly-empty-init-modules`] is enabled, all code, including imports, will be
/// flagged:
///
/// ```python
/// from submodule import MyClass
///
/// __all__ = ["MyClass"]
/// ```
///
/// And the only fix is entirely emptying the file:
///
/// ```python
/// ```
///
/// ## Options
/// - [`lint.ruff.strictly-empty-init-modules`]
///
/// ## References
/// - [`flake8-empty-init-modules`](https://github.com/samueljsb/flake8-empty-init-modules/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct NonEmptyInitModule;

impl Violation for NonEmptyInitModule {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Code detected".to_string()
    }
}

/// RUF070
pub(crate) fn non_empty_init_module(checker: &Checker, stmt: &ast::Stmt) {
    if !checker.in_init_module() {
        return;
    }

    // Only flag top-level statements
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    if !checker.settings().ruff.strictly_empty_init_modules {
        if checker.semantic().in_pep_257_docstring() || checker.semantic().in_attribute_docstring()
        {
            return;
        }

        if matches!(stmt, ast::Stmt::Import(_) | ast::Stmt::ImportFrom(_)) {
            return;
        }

        // Allow assignments to `__all__`.
        //
        // TODO(brent) should we allow additional cases here? Beyond simple assignments, you could
        // also append or extend `__all__`.
        //
        // This is actually going slightly beyond the upstream rule already, which only checks for
        // `Stmt::Assign`.
        let targets = match stmt {
            ast::Stmt::Assign(ast::StmtAssign { targets, .. }) => targets.as_slice(),
            ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, .. })
            | ast::Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => {
                std::slice::from_ref(&**target)
            }
            _ => &[],
        };

        if !targets.is_empty()
            && targets.iter().all(|target| {
                target
                    .as_name_expr()
                    .is_some_and(|name| name.id == "__all__")
            })
        {
            return;
        }
    }

    checker.report_diagnostic(NonEmptyInitModule, stmt.range());
}
