//! Check for imports of or from suspicious modules.
//!
//! See: <https://bandit.readthedocs.io/en/latest/blacklists/blacklist_imports.html>
use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprCall, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

// TODO: Violations and Docs

/// S401.py, S402, S403, S404, S405, S406, S407, S408, S409, S410, S411, S412, S413
pub(crate) fn suspicious_imports(checker: &mut Checker, stmt: &Stmt) {
    let Some(diagnostic_kind) = match stmt {
        // TODO: Implementation
        Stmt::Import(ast::StmtImport { names, range: _ }) => {},
        Stmt::ImportFrom(ast::StmtImport { names, range: _ }) => {},
        _ => panic!("Expected Stmt::Import | Stmt::ImportFrom")
    };

    let diagnostic = Diagnostic::new::<DiagnosticKind>(diagnostic_kind, stmt.range());
    if checker.enabled(diagnostic.kind.rule()) {
        checker.diagnostics.push(diagnostic);
    }
}
