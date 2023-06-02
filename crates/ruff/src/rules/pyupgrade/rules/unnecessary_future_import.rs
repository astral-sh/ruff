use itertools::Itertools;
use rustpython_parser::ast::{Alias, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct UnnecessaryFutureImport {
    pub names: Vec<String>,
}

impl AlwaysAutofixableViolation for UnnecessaryFutureImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryFutureImport { names } = self;
        if names.len() == 1 {
            let import = &names[0];
            format!("Unnecessary `__future__` import `{import}` for target Python version")
        } else {
            let imports = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Unnecessary `__future__` imports {imports} for target Python version")
        }
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary `__future__` import".to_string()
    }
}

const PY33_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
];

const PY37_PLUS_REMOVE_FUTURES: &[&str] = &[
    "nested_scopes",
    "generators",
    "with_statement",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
    "generator_stop",
];

/// UP010
pub(crate) fn unnecessary_future_import(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    let mut unused_imports: Vec<&Alias> = vec![];
    for alias in names {
        if alias.asname.is_some() {
            continue;
        }
        if PY33_PLUS_REMOVE_FUTURES.contains(&alias.name.as_str())
            || PY37_PLUS_REMOVE_FUTURES.contains(&alias.name.as_str())
        {
            unused_imports.push(alias);
        }
    }

    if unused_imports.is_empty() {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        UnnecessaryFutureImport {
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
            let unused_imports: Vec<String> = unused_imports
                .iter()
                .map(|alias| format!("__future__.{}", alias.name))
                .collect();
            let stmt = checker.semantic_model().stmt();
            let parent = checker.semantic_model().stmt_parent();
            let edit = autofix::edits::remove_unused_imports(
                unused_imports.iter().map(String::as_str),
                stmt,
                parent,
                checker.locator,
                checker.indexer,
                checker.stylist,
            )?;
            Ok(Fix::suggested(edit).isolate(checker.isolation(parent)))
        });
    }
    checker.diagnostics.push(diagnostic);
}
