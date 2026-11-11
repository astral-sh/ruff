use ruff_python_ast::Alias;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::resolve_imported_module_path;
use ruff_python_semantic::ModuleKind;
use ruff_text_size::Ranged;

use crate::codes::Category;
use crate::preview::is_import_self_from_module_enabled;
use crate::{Violation, checkers::ast::Checker};

/// ## What it does
/// Checks for import statements that import the current module.
///
/// ## Why is this bad?
/// Importing a module from itself is a circular dependency and results
/// in an `ImportError` exception.
///
/// In [preview], this rule also flags imports of names from the current
/// module, such as `from pkg.this_file import foo` or `from .this_file import foo`
/// in `pkg/this_file.py`. These aren't flagged in a package's `__init__.py`,
/// where `from pkg import foo` usually imports the submodule `pkg/foo.py`.
///
/// ## Example
///
/// ```python
/// # file: pkg/this_file.py
/// import pkg.this_file
/// from pkg import this_file
/// ```
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.265", category = Category::Suspicious)]
pub(crate) struct ImportSelf {
    name: String,
}

impl Violation for ImportSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Module `{name}` imports itself")
    }
}

/// PLW0406
pub(crate) fn import_self(checker: &Checker, alias: &Alias, module_path: Option<&[String]>) {
    let Some(module_path) = module_path else {
        return;
    };

    if alias.name.split('.').eq(module_path) {
        checker.report_diagnostic(
            ImportSelf {
                name: alias.name.to_string(),
            },
            alias.range(),
        );
    }
}

/// PLW0406
pub(crate) fn import_from_self(
    checker: &Checker,
    level: u32,
    module: Option<&str>,
    names: &[Alias],
    module_path: Option<&[String]>,
) {
    let Some(module_path) = module_path else {
        return;
    };
    let Some(imported_module_path) = resolve_imported_module_path(level, module, Some(module_path))
    else {
        return;
    };

    // `from pkg.mod import foo` or `from .mod import foo`, in `pkg/mod.py`. Every imported
    // name is reported, matching pylint.
    //
    // This is skipped in a package's `__init__.py`, where `from pkg import foo` usually imports
    // the submodule `pkg/foo.py` rather than a name defined in `pkg/__init__.py`, and the two
    // cases can't be told apart here.
    if is_import_self_from_module_enabled(checker.settings())
        && !matches!(checker.module.kind, ModuleKind::Package)
        && imported_module_path.split('.').eq(module_path)
    {
        for alias in names {
            checker.report_diagnostic(
                ImportSelf {
                    name: imported_module_path.to_string(),
                },
                alias.range(),
            );
        }
        return;
    }

    // `from pkg import mod` or `from . import mod`, in `pkg/mod.py`.
    if imported_module_path
        .split('.')
        .eq(&module_path[..module_path.len() - 1])
    {
        if let Some(alias) = names
            .iter()
            .find(|alias| alias.name == module_path[module_path.len() - 1])
        {
            checker.report_diagnostic(
                ImportSelf {
                    name: format!("{}.{}", imported_module_path, alias.name),
                },
                alias.range(),
            );
        }
    }
}
