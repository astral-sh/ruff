use itertools::Itertools;
use rustpython_parser::ast::{Alias, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for unnecessary imports of builtins.
///
/// ## Why is this bad?
/// Builtins are always available. Importing them is unnecessary and should be
/// removed to avoid confusion.
///
/// ## Example
/// ```python
/// from builtins import str
///
/// str(1)
/// ```
///
/// Use instead:
/// ```python
/// str(1)
/// ```
///
/// ## References
/// - [Python documentation: The Python Standard Library](https://docs.python.org/3/library/index.html)
#[violation]
pub struct UnnecessaryBuiltinImport {
    pub names: Vec<String>,
}

impl AlwaysAutofixableViolation for UnnecessaryBuiltinImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryBuiltinImport { names } = self;
        if names.len() == 1 {
            let import = &names[0];
            format!("Unnecessary builtin import: `{import}`")
        } else {
            let imports = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Unnecessary builtin imports: {imports}")
        }
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary builtin import".to_string()
    }
}

/// UP029
pub(crate) fn unnecessary_builtin_import(
    checker: &mut Checker,
    stmt: &Stmt,
    module: &str,
    names: &[Alias],
) {
    // Ignore irrelevant modules.
    if !matches!(
        module,
        "builtins" | "io" | "six" | "six.moves" | "six.moves.builtins"
    ) {
        return;
    }

    // Identify unaliased, builtin imports.
    let unused_imports: Vec<&Alias> = names
        .iter()
        .filter(|alias| alias.asname.is_none())
        .filter(|alias| {
            matches!(
                (module, alias.name.as_str()),
                (
                    "builtins" | "six.moves.builtins",
                    "*" | "ascii"
                        | "bytes"
                        | "chr"
                        | "dict"
                        | "filter"
                        | "hex"
                        | "input"
                        | "int"
                        | "isinstance"
                        | "list"
                        | "map"
                        | "max"
                        | "min"
                        | "next"
                        | "object"
                        | "oct"
                        | "open"
                        | "pow"
                        | "range"
                        | "round"
                        | "str"
                        | "super"
                        | "zip"
                ) | ("io", "open")
                    | ("six", "callable" | "next")
                    | ("six.moves", "filter" | "input" | "map" | "range" | "zip")
            )
        })
        .collect();

    if unused_imports.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryBuiltinImport {
            names: unused_imports
                .iter()
                .map(|alias| alias.name.to_string())
                .sorted()
                .collect(),
        },
        stmt.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            let stmt = checker.semantic().stmt();
            let parent = checker.semantic().stmt_parent();
            let unused_imports: Vec<String> = unused_imports
                .iter()
                .map(|alias| format!("{module}.{}", alias.name))
                .collect();
            let edit = autofix::edits::remove_unused_imports(
                unused_imports.iter().map(String::as_str),
                stmt,
                parent,
                checker.locator,
                checker.stylist,
                checker.indexer,
            )?;
            Ok(Fix::suggested(edit).isolate(checker.isolation(parent)))
        });
    }
    checker.diagnostics.push(diagnostic);
}
