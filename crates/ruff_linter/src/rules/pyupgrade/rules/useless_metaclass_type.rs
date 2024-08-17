use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for the use of `__metaclass__ = type` in class definitions.
///
/// ## Why is this bad?
/// Since Python 3, `__metaclass__ = type` is implied and can thus be omitted.
///
/// ## Example
///
/// ```python
/// class Foo:
///     __metaclass__ = type
/// ```
///
/// Use instead:
///
/// ```python
/// class Foo: ...
/// ```
///
/// ## References
/// - [PEP 3115](https://www.python.org/dev/peps/pep-3115/)
#[violation]
pub struct UselessMetaclassType;

impl AlwaysFixableViolation for UselessMetaclassType {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__metaclass__ = type` is implied")
    }

    fn fix_title(&self) -> String {
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
    let [Expr::Name(ast::ExprName { id, .. })] = targets else {
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
    let stmt = checker.semantic().current_statement();
    let parent = checker.semantic().current_statement_parent();
    let edit = fix::edits::delete_stmt(stmt, parent, checker.locator(), checker.indexer());
    diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
        checker.semantic().current_statement_parent_id(),
    )));
    checker.diagnostics.push(diagnostic);
}
