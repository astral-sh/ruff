use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::PySourceType;
use ruff_python_stdlib::identifiers::{
    is_identifier, is_migration_module_name, is_migration_name, is_module_name,
};
use ruff_python_stdlib::path::is_module_file;
use ruff_text_size::TextRange;

use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::package::PackageRoot;
use crate::rules::pep8_naming::settings::IgnoreNames;

/// ## What it does
/// Checks for module names that cannot be used in a regular `import`
/// statement because they are not valid Python identifiers.
///
/// ## Why is this bad?
/// For a module to be importable with an `import` statement, its name must be
/// a valid identifier. As such, module names cannot start with a digit,
/// contain spaces or dashes, or collide with hard keywords, like `import` or
/// `class`.
///
/// While such modules can still be loaded through other means, like
/// `importlib`, they cannot be referenced in `import` statements.
///
/// Module names that are valid identifiers but do not follow the `snake_case`
/// naming convention are flagged by [`non-snake-case-module-name`][N998]
/// instead.
///
/// ## Example
/// - Instead of `example-module-name` or `example module name`, use
///   `example_module_name`.
///
/// ## Options
///
/// - `lint.pep8-naming.ignore-names`
///
/// [N998]: https://docs.astral.sh/ruff/rules/non-snake-case-module-name/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.248", category = Category::Style)]
pub(crate) struct InvalidModuleName {
    name: String,
}

impl Violation for InvalidModuleName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidModuleName { name } = self;
        format!("Invalid module name: '{name}'")
    }
}

/// ## What it does
/// Checks for module names that do not follow the `snake_case` naming
/// convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends the use of the `snake_case` naming convention for
/// module names:
///
/// > Modules should have short, all-lowercase names. Underscores can be used in the
/// > module name if it improves readability. Python packages should also have short,
/// > all-lowercase names, although the use of underscores is discouraged.
/// >
/// > When an extension module written in C or C++ has an accompanying Python module that
/// > provides a higher level (e.g. more object-oriented) interface, the C/C++ module has
/// > a leading underscore (e.g. `_socket`).
///
/// Module names that are not valid Python identifiers at all, and so cannot be
/// used in a regular `import` statement, are flagged by
/// [`invalid-module-name`][N999] instead.
///
/// ## Example
/// - Instead of `ExampleModule`, use `example_module`.
///
/// ## Options
///
/// - `lint.pep8-naming.ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#package-and-module-names
/// [N999]: https://docs.astral.sh/ruff/rules/invalid-module-name/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Pedantic)]
pub(crate) struct NonSnakeCaseModuleName {
    name: String,
}

impl Violation for NonSnakeCaseModuleName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonSnakeCaseModuleName { name } = self;
        format!("Module name `{name}` should be snake_case")
    }
}

/// N999
pub(crate) fn invalid_module_name(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    ignore_names: &IgnoreNames,
    context: &LintContext,
) {
    let Some(module_name) = module_name(path, package) else {
        return;
    };

    // As a special case, we allow files in `versions` and `migrations` directories to start
    // with a digit (e.g., `0001_initial.py`), to support common conventions used by Django
    // and other frameworks.
    let is_importable = if is_migration_file(path) {
        is_migration_module_name(&module_name)
    } else {
        is_identifier(&module_name)
    };

    if !is_importable {
        // Ignore any explicitly-allowed names.
        if ignore_names.matches(&module_name) {
            return;
        }
        context.report_diagnostic(
            InvalidModuleName {
                name: module_name.into_owned(),
            },
            TextRange::default(),
        );
    }
}

/// N998
pub(crate) fn non_snake_case_module_name(
    path: &Path,
    package: Option<PackageRoot<'_>>,
    ignore_names: &IgnoreNames,
    context: &LintContext,
) {
    let Some(module_name) = module_name(path, package) else {
        return;
    };

    let is_migration_file = is_migration_file(path);

    // Module names that cannot be used in an `import` statement at all are
    // flagged by `invalid-module-name` instead.
    let is_importable = if is_migration_file {
        is_migration_module_name(&module_name)
    } else {
        is_identifier(&module_name)
    };
    if !is_importable {
        return;
    }

    let is_snake_case = if is_migration_file {
        is_migration_name(&module_name)
    } else {
        is_module_name(&module_name)
    };
    if is_snake_case {
        return;
    }

    // Ignore any explicitly-allowed names.
    if ignore_names.matches(&module_name) {
        return;
    }
    context.report_diagnostic(
        NonSnakeCaseModuleName {
            name: module_name.into_owned(),
        },
        TextRange::default(),
    );
}

/// Returns the name under which the file at `path` is imported, if the file
/// is a Python module within a package.
///
/// `__init__.py` and `__main__.py` files are imported under the name of the
/// parent directory rather than their file stem.
fn module_name<'a>(path: &'a Path, package: Option<PackageRoot<'a>>) -> Option<Cow<'a, str>> {
    if !PySourceType::try_from_path(path).is_some_and(PySourceType::is_py_file_or_stub) {
        return None;
    }

    let package = package?;

    if is_module_file(path) {
        package
            .path()
            .file_name()
            .map(|name| name.to_string_lossy())
    } else {
        path.file_stem().map(|stem| stem.to_string_lossy())
    }
}

/// Return `true` if a [`Path`] refers to a migration file.
fn is_migration_file(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .is_some_and(|parent| matches!(parent, "versions" | "migrations"))
}
