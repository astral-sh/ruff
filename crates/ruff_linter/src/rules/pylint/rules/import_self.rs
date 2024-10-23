use ruff_python_ast::Alias;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::resolve_imported_module_path;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for import statements that import the current module.
///
/// ## Why is this bad?
/// Importing a module from itself is a circular dependency.
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
#[violation]
pub struct ImportSelf {
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
pub(crate) fn import_self(alias: &Alias, module_path: Option<&[String]>) -> Option<Diagnostic> {
    let module_path = module_path?;

    if alias.name.split('.').eq(module_path) {
        return Some(Diagnostic::new(
            ImportSelf {
                name: alias.name.to_string(),
            },
            alias.range(),
        ));
    }

    None
}

/// PLW0406
pub(crate) fn import_from_self(
    level: u32,
    module: Option<&str>,
    names: &[Alias],
    module_path: Option<&[String]>,
) -> Option<Diagnostic> {
    let module_path = module_path?;
    let imported_module_path = resolve_imported_module_path(level, module, Some(module_path))?;

    if imported_module_path
        .split('.')
        .eq(&module_path[..module_path.len() - 1])
    {
        if let Some(alias) = names
            .iter()
            .find(|alias| alias.name == module_path[module_path.len() - 1])
        {
            return Some(Diagnostic::new(
                ImportSelf {
                    name: format!("{}.{}", imported_module_path, alias.name),
                },
                alias.range(),
            ));
        }
    }

    None
}
