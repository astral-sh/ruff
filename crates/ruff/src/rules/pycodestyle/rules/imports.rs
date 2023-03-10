use rustpython_parser::ast::{Alias, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for multiple imports on one line.
///
/// ## Why is this bad?
/// Place imports on separate lines.
///
/// The following are okay:
/// ```python
/// from subprocess import Popen, PIPE
/// from myclas import MyClass
/// from foo.bar.yourclass import YourClass
/// import myclass
/// import foo.bar.yourclass
/// ```
///
/// ## Example
/// ```python
/// import sys, os
/// ```
///
/// Use instead:
/// ```python
/// import os
/// import sys
/// ```
#[violation]
pub struct MultipleImportsOnOneLine;

impl Violation for MultipleImportsOnOneLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple imports on one line")
    }
}

/// ## What it does
///
///
/// ## Why is this bad?
/// Place imports at the top of the file.
///
/// Always put imports at the top of the file, just after any module
/// comments and docstrings, and before module globals and constants.
///
/// Exceptions:
/// ```python
/// # this is a comment
/// import os
/// ```
///
/// ```python
/// '''this is a module docstring'''
/// import os
/// ```
///
/// ```python
/// r'''this is a module docstring'''
/// import os
/// ```
///
/// ```python
/// try:
///     import x
/// except ImportError:
///     pass
/// else:
///     pass
/// import y
/// ```
///
/// ```python
/// try:
///     import x
/// except ImportError:
///     pass
/// finally:
///     pass
/// import y
/// ```
///
/// ## Example
/// ```python
/// 'One string'
/// "Two string"
/// a = 1
/// import os
/// from sys import x
/// ```
///
/// Use instead:
/// ```python
/// import os
/// from sys import x
/// 'One string'
/// "Two string"
/// a = 1
/// ```
#[violation]
pub struct ModuleImportNotAtTopOfFile;

impl Violation for ModuleImportNotAtTopOfFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Module level import not at top of file")
    }
}

/// E401
pub fn multiple_imports_on_one_line(checker: &mut Checker, stmt: &Stmt, names: &[Alias]) {
    if names.len() > 1 {
        checker
            .diagnostics
            .push(Diagnostic::new(MultipleImportsOnOneLine, Range::from(stmt)));
    }
}

/// E402
pub fn module_import_not_at_top_of_file(checker: &mut Checker, stmt: &Stmt) {
    if checker.ctx.seen_import_boundary && stmt.location.column() == 0 {
        checker.diagnostics.push(Diagnostic::new(
            ModuleImportNotAtTopOfFile,
            Range::from(stmt),
        ));
    }
}
