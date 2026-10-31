use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::{Expr, Stmt};
use ruff_python_semantic::analyze::typing::is_type_checking_block;
use ruff_text_size::Ranged;

use crate::codes::Category;
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
/// """My module docstring."""
///
/// from submodule import MyClass
///
/// __all__ = ["MyClass"]
/// ```
///
/// the only fix is removing everything but the docstring:
///
/// ```python
/// """My module docstring."""
/// ```
///
/// ## Details
///
/// In both modes, this rule allows module-level and attribute docstrings in `__init__.py` files.
///
/// In non-strict mode, this rule additionally allows several common patterns:
///
/// - Imports
/// - Assignments to dunder names (identifiers starting and ending with `__`, such as `__all__` or
///   `__submodules__`)
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
#[violation_metadata(preview_since = "0.14.11", category = Category::Pedantic)]
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
pub(crate) fn non_empty_init_module(checker: &Checker, stmt: flat::View<'_, Stmt>) {
    if !checker.in_init_module() {
        return;
    }

    let semantic = checker.semantic();

    // Only flag top-level statements
    if !semantic.at_top_level() {
        return;
    }

    // Docstrings are documentation rather than code, so they're allowed in both modes. Even though
    // module-level attributes are disallowed, we still allow attribute docstrings to avoid needing
    // two `noqa` comments in a case like:
    //
    // ```py
    // MY_CONSTANT = 1  # noqa: RUF067
    // "A very important constant"
    // ```
    if semantic.in_pep_257_docstring() || semantic.in_attribute_docstring() {
        return;
    }

    let strictly_empty_init_modules = checker.settings().ruff.strictly_empty_init_modules;

    if !strictly_empty_init_modules {
        match stmt.classify() {
            // Allow imports
            flat::StmtKind::Import(_) | flat::StmtKind::ImportFrom(_) => return,

            // Allow PEP-562 module `__getattr__` and `__dir__`
            flat::StmtKind::FunctionDef(func)
                if matches!(func.name().as_str(), "__getattr__" | "__dir__") =>
            {
                return;
            }

            // Allow `TYPE_CHECKING` blocks
            flat::StmtKind::If(stmt_if) if is_type_checking_block(stmt_if, semantic) => return,

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
    targets: itertools::Either<
        flat::ListIter<'a, flat::Handle<Expr>>,
        std::iter::Once<flat::View<'a, Expr>>,
    >,
}

impl<'a> Assignment<'a> {
    fn from_stmt(stmt: flat::View<'a, Stmt>) -> Option<Self> {
        let targets = match stmt.classify() {
            flat::StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();

                itertools::Either::Left(targets.iter())
            }
            flat::StmtKind::AnnAssign(stmt_ann_assign) => {
                let target = stmt_ann_assign.target();

                itertools::Either::Right(std::iter::once(target))
            }
            flat::StmtKind::AugAssign(stmt_aug_assign) => {
                let target = stmt_aug_assign.target();

                itertools::Either::Right(std::iter::once(target))
            }
            _ => return None,
        };

        Some(Self { targets })
    }

    /// Returns `true` when every assignment target is a simple name and each name is a dunder
    /// (`__` prefix and suffix), matching [`is_dunder`].
    fn all_targets_are_dunder(&self) -> bool {
        self.targets.clone().all(|target| {
            target
                .as_name_expr()
                .is_some_and(|name| is_dunder(name.id().as_str()))
        })
    }
}
