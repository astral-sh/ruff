use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Alias;

use crate::checkers::ast::Checker;
use crate::rules::flake8_builtins::helpers::shadows_builtin;

/// ## What it does
/// Checks for imports that use the same names as builtins.
///
/// ## Why is this bad?
/// Reusing a builtin for the name of an import increases the difficulty
/// of reading and maintaining the code, and can cause non-obvious errors,
/// as readers may mistake the variable for the builtin and vice versa.
///
/// Builtins can be marked as exceptions to this rule via the
/// [`lint.flake8-builtins.builtins-ignorelist`] configuration option.
///
/// ## Options
/// - `lint.flake8-builtins.builtins-ignorelist`
#[violation]
pub struct BuiltinImportShadowing {
    name: String,
}

impl Violation for BuiltinImportShadowing {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BuiltinImportShadowing { name } = self;
        format!("Import `{name}` is shadowing a Python builtin")
    }
}

/// A004
pub(crate) fn builtin_import_shadowing(checker: &mut Checker, alias: &Alias) {
    let name = alias.asname.as_ref().unwrap_or(&alias.name);
    if shadows_builtin(
        name.as_str(),
        checker.source_type,
        &checker.settings.flake8_builtins.builtins_ignorelist,
        checker.settings.target_version,
    ) {
        checker.diagnostics.push(Diagnostic::new(
            BuiltinImportShadowing {
                name: name.to_string(),
            },
            name.range,
        ));
    }
}
