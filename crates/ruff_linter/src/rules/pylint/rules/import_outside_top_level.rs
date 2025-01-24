use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Alias, Stmt};
use ruff_text_size::Ranged;

use crate::{
    checkers::ast::Checker,
    codes::Rule,
    rules::flake8_tidy_imports::matchers::{MatchName, MatchNameOrParent, NameMatchPolicy},
};

/// ## What it does
/// Checks for `import` statements outside of a module's top-level scope, such
/// as within a function or class definition.
///
/// ## Why is this bad?
/// [PEP 8] recommends placing imports not only at the top-level of a module,
/// but at the very top of the file, "just after any module comments and
/// docstrings, and before module globals and constants."
///
/// `import` statements have effects that are global in scope; defining them at
/// the top level has a number of benefits. For example, it makes it easier to
/// identify the dependencies of a module, and ensures that any invalid imports
/// are caught regardless of whether a specific function is called or class is
/// instantiated.
///
/// An import statement would typically be placed within a function only to
/// avoid a circular dependency, to defer a costly module load, or to avoid
/// loading a dependency altogether in a certain runtime environment.
///
/// ## Example
/// ```python
/// def print_python_version():
///     import platform
///
///     print(python.python_version())
/// ```
///
/// Use instead:
/// ```python
/// import platform
///
///
/// def print_python_version():
///     print(python.python_version())
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#imports
#[derive(ViolationMetadata)]
pub(crate) struct ImportOutsideTopLevel;

impl Violation for ImportOutsideTopLevel {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`import` should be at the top-level of a file".to_string()
    }
}

/// C0415
pub(crate) fn import_outside_top_level(
    checker: &mut Checker,
    stmt: &Stmt,
    pkg: Option<&str>,
    modules: &[Alias],
) {
    if checker.semantic().current_scope().kind.is_module() {
        // "Top-level" imports are allowed
        return;
    }

    // Check if any of the non-top-level imports are banned by TID253
    // before emitting the diagnostic
    if checker.enabled(Rule::BannedModuleLevelImports) {
        match pkg {
            // import from a package
            Some(pkg) => {
                // Either the whole package is banned
                if NameMatchPolicy::MatchNameOrParent(
                    MatchNameOrParent {
                        module: pkg,
                    }
                ).find(
                        checker
                            .settings
                            .flake8_tidy_imports
                            .banned_module_level_imports
                            .iter()
                            .map(AsRef::as_ref),
                    ).is_some()
                // Or all of the import modules are banned
                || modules.iter()
                .map(|alias| {
                    NameMatchPolicy::MatchName(
                        MatchName {
                            module: pkg,
                            member: &alias.name,
                        },
                    )
                }).all(|policy| {
                    policy
                        .find(
                            checker
                                .settings
                                .flake8_tidy_imports
                                .banned_module_level_imports
                                .iter()
                                .map(AsRef::as_ref),
                        )
                        .is_some()
                }) {
                    return;
                }
            }
            None => {
                // plain import
                if modules
                    .iter()
                    .map(|alias| {
                        NameMatchPolicy::MatchNameOrParent(MatchNameOrParent {
                            module: &alias.name,
                        })
                    })
                    .all(|policy| {
                        policy
                            .find(
                                checker
                                    .settings
                                    .flake8_tidy_imports
                                    .banned_module_level_imports
                                    .iter()
                                    .map(AsRef::as_ref),
                            )
                            .is_some()
                    })
                {
                    return;
                }
            }
        }
    }

    // Emit the diagnostic
    checker
        .diagnostics
        .push(Diagnostic::new(ImportOutsideTopLevel, stmt.range()));
}
