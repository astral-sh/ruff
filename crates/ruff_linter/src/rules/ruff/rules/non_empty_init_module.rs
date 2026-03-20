use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::typing::is_type_checking_block;
use ruff_text_size::Ranged;

use crate::{Violation, checkers::ast::Checker};

/// ## What it does
///
/// Detects the presence of code in `__init__.py` files.
///
/// ## Why is this bad?
///
/// `__init__.py` files are often empty or only contain simple code to modify a module's API. As
/// such, it's easy to overlook them and their possible side effects when debugging.
///
/// ## Example
///
/// Instead of defining `MyClass` directly in `__init__.py`:
///
/// ```python
/// """My module docstring."""
///
///
/// class MyClass:
///     def my_method(self): ...
/// ```
///
/// move the definition to another file, import it, and include it in `__all__`:
///
/// ```python
/// """My module docstring."""
///
/// from submodule import MyClass
///
/// __all__ = ["MyClass"]
/// ```
///
/// Code in `__init__.py` files is also run at import time and can cause surprising slowdowns. To
/// disallow any code in `__init__.py` files, you can enable the
/// [`lint.ruff.strictly-empty-init-modules`] setting. In this case:
///
/// ```python
/// from submodule import MyClass
///
/// __all__ = ["MyClass"]
/// ```
///
/// the only fix is entirely emptying the file:
///
/// ```python
/// ```
///
/// ## Details
///
/// In non-strict mode, this rule allows several common patterns in `__init__.py` files:
///
/// - Imports
/// - Assignments to dunder names (identifiers starting and ending with `__`, such as `__all__` or
///   `__submodules__`)
/// - Module-level and attribute docstrings
/// - `if TYPE_CHECKING` blocks
/// - [PEP-562] module-level `__getattr__` and `__dir__` functions
///
/// ## Options
///
/// - [`lint.ruff.strictly-empty-init-modules`]
///
/// ## References
///
/// - [`flake8-empty-init-modules`](https://github.com/samueljsb/flake8-empty-init-modules/)
///
/// [PEP-562]: https://peps.python.org/pep-0562/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct NonEmptyInitModule {
    strictly_empty_init_modules: bool,
}

impl Violation for NonEmptyInitModule {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.strictly_empty_init_modules {
            "`__init__` module should not contain any code".to_string()
        } else {
            "`__init__` module should only contain docstrings and re-exports".to_string()
        }
    }
}

/// RUF067
pub(crate) fn non_empty_init_module(checker: &Checker, stmt: &Stmt) {
    if !checker.in_init_module() {
        return;
    }

    let semantic = checker.semantic();

    // Only flag top-level statements
    if !semantic.at_top_level() {
        return;
    }

    let strictly_empty_init_modules = checker.settings().ruff.strictly_empty_init_modules;

    if !strictly_empty_init_modules {
        // Even though module-level attributes are disallowed, we still allow attribute docstrings
        // to avoid needing two `noqa` comments in a case like:
        //
        // ```py
        // MY_CONSTANT = 1  # noqa: RUF067
        // "A very important constant"
        // ```
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

        if let Some(assignment) = Assignment::from_stmt(stmt) {
            // Allow assignments to any dunder-named target (e.g. `__all__`, `__path__`, or
            // tool-specific names like `__submodules__`). Chained assignments require every target
            // to be a dunder.
            if assignment.all_targets_are_dunder() {
                return;
            }
        }
    }

    checker.report_diagnostic(
        NonEmptyInitModule {
            strictly_empty_init_modules,
        },
        stmt.range(),
    );
}

/// Any assignment statement, including plain assignment, annotated assignments, and augmented
/// assignments.
struct Assignment<'a> {
    targets: &'a [Expr],
}

impl<'a> Assignment<'a> {
    fn from_stmt(stmt: &'a Stmt) -> Option<Self> {
        let targets = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => targets.as_slice(),
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => std::slice::from_ref(&**target),
            Stmt::AugAssign(ast::StmtAugAssign { target, .. }) => std::slice::from_ref(&**target),
            _ => return None,
        };

        Some(Self { targets })
    }

    /// Returns `true` when every assignment target is a simple name and each name is a dunder
    /// (`__` prefix and suffix), matching [`is_dunder`].
    fn all_targets_are_dunder(&self) -> bool {
        !self.targets.is_empty()
            && self.targets.iter().all(|target| {
                target
                    .as_name_expr()
                    .is_some_and(|name| is_dunder(name.id.as_str()))
            })
    }
}
