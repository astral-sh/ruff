use ruff_macros::{ViolationMetadata, derive_message_formats};
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
/// - Assignments to `__all__`, `__path__`, `__version__`, and `__author__`
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
            // Allow assignments to `__all__`.
            //
            // TODO(brent) should we allow additional cases here? Beyond simple assignments, you could
            // also append or extend `__all__`.
            //
            // This is actually going slightly beyond the upstream rule already, which only checks for
            // `Stmt::Assign`.
            if assignment.is_assignment_to("__all__") {
                return;
            }

            // Allow legacy namespace packages with assignments like:
            //
            // ```py
            // __path__ = __import__('pkgutil').extend_path(__path__, __name__)
            // ```
            if assignment.is_assignment_to("__path__") && assignment.is_pkgutil_extend_path() {
                return;
            }

            // Allow assignments to `__version__`.
            if assignment.is_assignment_to("__version__") {
                return;
            }

            // Allow assignments to `__author__`.
            if assignment.is_assignment_to("__author__") {
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
    value: Option<&'a Expr>,
}

impl<'a> Assignment<'a> {
    fn from_stmt(stmt: &'a Stmt) -> Option<Self> {
        let (targets, value) = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                (targets.as_slice(), Some(&**value))
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                (std::slice::from_ref(&**target), value.as_deref())
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                (std::slice::from_ref(&**target), Some(&**value))
            }
            _ => return None,
        };

        Some(Self { targets, value })
    }

    /// Returns whether all of the assignment targets match `name`.
    ///
    /// For example, both of the following would be allowed for a `name` of `__all__`:
    ///
    /// ```py
    /// __all__ = ["foo"]
    /// __all__ = __all__ = ["foo"]
    /// ```
    ///
    /// but not:
    ///
    /// ```py
    /// __all__ = another_list = ["foo"]
    /// ```
    fn is_assignment_to(&self, name: &str) -> bool {
        self.targets
            .iter()
            .all(|target| target.as_name_expr().is_some_and(|expr| expr.id == name))
    }

    /// Returns `true` if the value being assigned is a call to `pkgutil.extend_path`.
    ///
    /// For example, both of the following would return true:
    ///
    /// ```py
    /// __path__ = __import__('pkgutil').extend_path(__path__, __name__)
    /// __path__ = other.extend_path(__path__, __name__)
    /// ```
    ///
    /// We're intentionally a bit less strict here, not requiring that the receiver of the
    /// `extend_path` call is the typical `__import__('pkgutil')` or `pkgutil`.
    fn is_pkgutil_extend_path(&self) -> bool {
        let Some(Expr::Call(ast::ExprCall {
            func: extend_func,
            arguments: extend_arguments,
            ..
        })) = self.value
        else {
            return false;
        };

        let Expr::Attribute(ast::ExprAttribute {
            attr: maybe_extend_path,
            ..
        }) = &**extend_func
        else {
            return false;
        };

        // Test that this is an `extend_path(__path__, __name__)` call
        if maybe_extend_path != "extend_path" {
            return false;
        }

        let Some(Expr::Name(path)) = extend_arguments.find_argument_value("path", 0) else {
            return false;
        };
        let Some(Expr::Name(name)) = extend_arguments.find_argument_value("name", 1) else {
            return false;
        };

        path.id() == "__path__" && name.id() == "__name__"
    }
}
