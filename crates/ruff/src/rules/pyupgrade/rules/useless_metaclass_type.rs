use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for the use of `__metaclass__ = type` in class definitions.
///
/// ## Why is this bad?
/// Since Python 3, `__metaclass__ = type` is implied and can thus be omitted.
///
/// ## Example
/// ```python
/// class Foo:
///     __metaclass__ = type
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     ...
/// ```
///
/// ## References
/// - [PEP 3115](https://www.python.org/dev/peps/pep-3115/)
#[violation]
pub struct UselessMetaclassType;

impl AlwaysAutofixableViolation for UselessMetaclassType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__metaclass__ = type` is implied")
    }

    fn autofix_title(&self) -> String {
        "Remove `__metaclass__ = type`".to_string()
    }
}

/// UP001
pub(crate) fn useless_metaclass_type(
    checker: &mut Checker,
    stmt: &Stmt,
    value: &Expr,
    targets: &[Expr],
) {
    if targets.len() != 1 {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. }) = targets.first().unwrap() else {
        return;
    };
    if id != "__metaclass__" {
        return;
    }
    let Expr::Name(ast::ExprName { id, .. }) = value else {
        return;
    };
    if id != "type" {
        return;
    }

    let mut diagnostic = Diagnostic::new(UselessMetaclassType, stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        let stmt = checker.semantic().stmt();
        let parent = checker.semantic().stmt_parent();
        let edit = autofix::edits::delete_stmt(stmt, parent, checker.locator, checker.indexer);
        diagnostic.set_fix(Fix::automatic(edit).isolate(checker.isolation(parent)));
    }
    checker.diagnostics.push(diagnostic);
}
