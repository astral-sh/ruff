use ruff_python_ast::Alias;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::resolve_imported_module_path;
use ruff_text_size::Ranged;

use crate::{Violation, checkers::ast::Checker};

/// ## What it does
/// Checks for import statements that import the current module.
///
/// ## Why is this bad?
/// Importing a module from itself is a circular dependency and results
/// in an `ImportError` exception.
///
/// ## Example
///
/// ```python
/// # file: this_file.py
/// from this_file import foo
///
///
/// def foo(): ...
/// ```
#[derive(ViolationMetadata)]
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
