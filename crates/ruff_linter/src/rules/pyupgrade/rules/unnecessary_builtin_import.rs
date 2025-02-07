use itertools::Itertools;
use ruff_python_ast::{Alias, Stmt};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

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
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryBuiltinImport {
    pub names: Vec<String>,
}

impl AlwaysFixableViolation for UnnecessaryBuiltinImport {
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

    fn fix_title(&self) -> String {
        "Remove unnecessary builtin import".to_string()
    }
}

/// UP029
pub(crate) fn unnecessary_builtin_import(
    checker: &Checker,
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
    diagnostic.try_set_fix(|| {
        let statement = checker.semantic().current_statement();
        let parent = checker.semantic().current_statement_parent();
        let edit = fix::edits::remove_unused_imports(
            unused_imports
                .iter()
                .map(|alias| &alias.name)
                .map(ruff_python_ast::Identifier::as_str),
            statement,
            parent,
            checker.locator(),
            checker.stylist(),
            checker.indexer(),
        )?;
        Ok(Fix::unsafe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_parent_id(),
        )))
    });
    checker.report_diagnostic(diagnostic);
}
