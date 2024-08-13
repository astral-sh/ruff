use itertools::Itertools;
use ruff_python_ast::{StmtImportFrom, StmtImportFromMemberList};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct UnnecessaryBuiltinImport {
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
pub(crate) fn unnecessary_builtin_import(checker: &mut Checker, import_from: &StmtImportFrom) {
    let Some(module) = import_from.module().as_deref() else {
        return;
    };

    // Ignore irrelevant modules.
    if !matches!(
        module,
        "builtins" | "io" | "six" | "six.moves" | "six.moves.builtins"
    ) {
        return;
    }

    let unused_imports = match import_from {
        StmtImportFrom::Star(_) => {
            if matches!(module, "builtins" | "six.moves.builtins") {
                vec!["*"]
            } else {
                return;
            }
        }
        StmtImportFrom::MemberList(StmtImportFromMemberList { names, .. }) => names
            .iter()
            .filter(|alias| alias.asname.is_none())
            .filter(|alias| {
                matches!(
                    (module, alias.name.as_str()),
                    (
                        "builtins" | "six.moves.builtins",
                        "ascii"
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
            .map(|alias| alias.name.as_str())
            .collect(),
    };

    if unused_imports.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryBuiltinImport {
            names: unused_imports
                .iter()
                .map(std::string::ToString::to_string)
                .sorted()
                .collect(),
        },
        import_from.range(),
    );
    diagnostic.try_set_fix(|| {
        let semantic = checker.semantic();
        let edit = fix::edits::remove_unused_imports(
            unused_imports,
            semantic.current_statement(),
            semantic.current_statement_parent(),
            checker.locator(),
            checker.stylist(),
            checker.indexer(),
        )?;
        Ok(Fix::unsafe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_parent_id(),
        )))
    });
    checker.diagnostics.push(diagnostic);
}
