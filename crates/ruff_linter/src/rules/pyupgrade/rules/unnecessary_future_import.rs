use itertools::Itertools;
use ruff_python_ast::{Alias, Stmt};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for unnecessary `__future__` imports.
///
/// ## Why is this bad?
/// The `__future__` module is used to enable features that are not yet
/// available in the current Python version. If a feature is already
/// available in the minimum supported Python version, importing it
/// from `__future__` is unnecessary and should be removed to avoid
/// confusion.
///
/// ## Example
/// ```python
/// from __future__ import print_function
///
/// print("Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world!")
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `__future__` — Future statement definitions](https://docs.python.org/3/library/__future__.html)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryFutureImport {
    pub names: Vec<String>,
}

impl AlwaysFixableViolation for UnnecessaryFutureImport {
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

    fn fix_title(&self) -> String {
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
pub(crate) fn unnecessary_future_import(checker: &Checker, stmt: &Stmt, names: &[Alias]) {
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
        Ok(Fix::safe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_parent_id(),
        )))
    });
    checker.report_diagnostic(diagnostic);
}
