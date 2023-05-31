use itertools::Itertools;
use log::error;
use rustpython_parser::ast::{Alias, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::RefEquality;

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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

const BUILTINS: &[&str] = &[
    "*",
    "ascii",
    "bytes",
    "chr",
    "dict",
    "filter",
    "hex",
    "input",
    "int",
    "isinstance",
    "list",
    "map",
    "max",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "pow",
    "range",
    "round",
    "str",
    "super",
    "zip",
];
const IO: &[&str] = &["open"];
const SIX_MOVES_BUILTINS: &[&str] = BUILTINS;
const SIX: &[&str] = &["callable", "next"];
const SIX_MOVES: &[&str] = &["filter", "input", "map", "range", "zip"];

/// UP029
pub(crate) fn unnecessary_builtin_import(
    checker: &mut Checker,
    stmt: &Stmt,
    module: &str,
    names: &[Alias],
) {
    let deprecated_names = match module {
        "builtins" => BUILTINS,
        "io" => IO,
        "six" => SIX,
        "six.moves" => SIX_MOVES,
        "six.moves.builtins" => SIX_MOVES_BUILTINS,
        _ => return,
    };

    let mut unused_imports: Vec<&Alias> = vec![];
    for alias in names {
        if alias.asname.is_some() {
            continue;
        }
        if deprecated_names.contains(&alias.name.as_str()) {
            unused_imports.push(alias);
        }
    }

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
        let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
        let defined_by = checker.semantic_model().stmt();
        let defined_in = checker.semantic_model().stmt_parent();
        let unused_imports: Vec<String> = unused_imports
            .iter()
            .map(|alias| format!("{module}.{}", alias.name))
            .collect();
        match autofix::edits::remove_unused_imports(
            unused_imports.iter().map(String::as_str),
            defined_by,
            defined_in,
            &deleted,
            checker.locator,
            checker.indexer,
            checker.stylist,
        ) {
            Ok(edit) => {
                if edit.is_deletion() || edit.content() == Some("pass") {
                    checker.deletions.insert(RefEquality(defined_by));
                }
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(edit));
            }
            Err(e) => error!("Failed to remove builtin import: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}
