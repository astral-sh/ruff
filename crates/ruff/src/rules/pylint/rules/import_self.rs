use rustpython_parser::ast::Alias;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::resolve_imported_module_path;

#[violation]
pub struct ImportSelf {
    pub name: String,
}

impl Violation for ImportSelf {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { name } = self;
        format!("Module `{name}` imports itself")
    }
}

/// PLW0406
pub fn import_self(alias: &Alias, module_path: Option<&[String]>) -> Option<Diagnostic> {
    let Some(module_path) = module_path else {
        return None;
    };

    if alias.node.name.split('.').eq(module_path) {
        return Some(Diagnostic::new(
            ImportSelf {
                name: alias.node.name.clone(),
            },
            alias.range(),
        ));
    }

    None
}

/// PLW0406
pub fn import_from_self(
    level: Option<usize>,
    module: Option<&str>,
    names: &[Alias],
    module_path: Option<&[String]>,
) -> Option<Diagnostic> {
    let Some(module_path) = module_path else {
        return None;
    };
    let Some(imported_module_path) = resolve_imported_module_path(level, module, Some(module_path)) else {
        return None;
    };

    if imported_module_path
        .split('.')
        .eq(&module_path[..module_path.len() - 1])
    {
        if let Some(alias) = names
            .iter()
            .find(|alias| alias.node.name == module_path[module_path.len() - 1])
        {
            return Some(Diagnostic::new(
                ImportSelf {
                    name: format!("{}.{}", imported_module_path, alias.node.name),
                },
                alias.range(),
            ));
        }
    }

    None
}
