use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use ruff_python_ast::{self as ast, Stmt};

/// ## What it does
/// Checks for incorrectly imported `trio`.
///
/// ## Why is this bad?
/// When trio is imported under a different name, the rules do not apply.
///
/// ## Example
/// ```python
/// import trio as t
/// ```
///
/// Use instead:
/// ```python
/// import trio
/// ```
#[violation]
pub struct TrioInvalidImportStyle;

impl Violation for TrioInvalidImportStyle {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("trio must be imported with import trio for the linter to work")
    }
}

pub(crate) fn invalid_trio_import(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, range: _ }) => {
            for name in names {
                if &name.name == "trio" && name.asname.is_some() {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(TrioInvalidImportStyle, name.range));
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom {
            module: Some(module),
            range,
            ..
        }) => {
            if module == "trio" {
                checker
                    .diagnostics
                    .push(Diagnostic::new(TrioInvalidImportStyle, *range));
            }
        }
        _ => {}
    };
}
