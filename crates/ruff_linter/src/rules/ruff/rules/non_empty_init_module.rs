use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_semantic::analyze::typing::is_type_checking_block;
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
/// When [`lint.ruff.strictly-empty-init-modules`] is enabled, all code, including imports, docstrings, and `__all__`, will be
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
pub(crate) fn non_empty_init_module(checker: &Checker, stmt: &Stmt) {
    if !checker.in_init_module() {
        return;
    }

    let semantic = checker.semantic();

    // Only flag top-level statements
    if !semantic.at_top_level() {
        return;
    }

    if !checker.settings().ruff.strictly_empty_init_modules {
        if semantic.in_pep_257_docstring() || semantic.in_attribute_docstring() {
            return;
        }

        match stmt {
            // Allow imports
            Stmt::Import(_) | Stmt::ImportFrom(_) => return,

            // Allow PEP-562 module `__getattr__` and `__dir__`
            Stmt::FunctionDef(func) if matches!(&*func.name, "__getattr__" | "__dir__") => return,

            // Allow `TYPE_CHECKING` blocks
            Stmt::If(stmt_if) if is_type_checking_block(stmt_if, semantic) => return,

            _ => {}
        }

        // Allow assignments to `__all__`.
        //
        // TODO(brent) should we allow additional cases here? Beyond simple assignments, you could
        // also append or extend `__all__`.
        //
        // This is actually going slightly beyond the upstream rule already, which only checks for
        // `Stmt::Assign`.
        if is_assignment_to(stmt, "__all__") {
            return;
        }

        // Allow legacy namespace packages with assignments like:
        //
        // ```py
        // __path__ = __import__('pkgutil').extend_path(__path__, __name__)
        // ```
        if is_assignment_to(stmt, "__path__") {
            return;
        }

        // Allow assignments to `__version__`.
        if is_assignment_to(stmt, "__version__") {
            return;
        }
    }

    checker.report_diagnostic(NonEmptyInitModule, stmt.range());
}

fn is_assignment_to(stmt: &Stmt, name: &str) -> bool {
    let targets = match stmt {
        Stmt::Assign(ast::StmtAssign { targets, .. }) => targets.as_slice(),
        Stmt::AnnAssign(ast::StmtAnnAssign { target, .. })
        | Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => std::slice::from_ref(&**target),
        _ => return false,
    };

    targets
        .iter()
        .all(|target| target.as_name_expr().is_some_and(|expr| expr.id == name))
}
