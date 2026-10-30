use ruff_python_ast::{Expr, Stmt, flat};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix;
use crate::{AlwaysFixableViolation, Fix};

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
/// - [PEP 3115 – Metaclasses in Python 3000](https://peps.python.org/pep-3115/)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155", category = Category::Style)]
pub(crate) struct UselessMetaclassType;

impl AlwaysFixableViolation for UselessMetaclassType {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`__metaclass__ = type` is implied".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `__metaclass__ = type`".to_string()
    }
}

/// UP001
pub(crate) fn useless_metaclass_type(
    checker: &Checker,
    stmt: flat::View<'_, Stmt>,
    value: flat::View<'_, Expr>,
    targets: flat::NodeList<'_, Expr>,
) {
    let Some([target]) = targets.as_array() else {
        return;
    };
    if target
        .as_name_expr()
        .is_none_or(|name| name.id() != "__metaclass__")
        || value.as_name_expr().is_none_or(|name| name.id() != "type")
    {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(UselessMetaclassType, stmt.range());
    let stmt = checker.semantic().current_statement();
    let parent = checker.semantic().current_statement_parent();
    let edit = fix::edits::delete_stmt(stmt, parent, checker.locator(), checker.indexer());
    diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
        checker.semantic().current_statement_parent_id(),
    )));
}
