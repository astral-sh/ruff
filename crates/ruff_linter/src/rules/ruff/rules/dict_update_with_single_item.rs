use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprAttribute};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for `dict.update({"key": "value"})` calls with a single-item
/// dictionary literal.
///
/// ## Why is this bad?
/// It's simpler and more efficient to use direct key assignment
/// (`d["key"] = "value"`) instead of calling `dict.update()` with a
/// single-item dictionary literal. The `dict.update()` variant
/// unnecessarily creates an intermediate dictionary object, and is harder
/// to read.
///
/// ## Example
///
/// ```python
/// d = {}
/// d.update({"key": "value"})
/// ```
///
/// Use instead:
///
/// ```python
/// d = {}
/// d["key"] = "value"
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe because `dict.update()` returns
/// `None`, and while it is almost always used as a statement, it could
/// theoretically appear in an expression context where replacing it with
/// an assignment would change the semantics.
///
/// ## References
/// - [Python documentation: `dict.update`](https://docs.python.org/3/library/stdtypes.html#dict.update)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.2")]
pub(crate) struct DictUpdateWithSingleItem;

impl Violation for DictUpdateWithSingleItem {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use direct key assignment instead of `dict.update()` with a single-item dictionary literal"
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with direct key assignment".to_string())
    }
}

/// RUF071
pub(crate) fn dict_update_with_single_item(checker: &Checker, call: &ast::ExprCall) {
    let Expr::Attribute(ExprAttribute { value, attr, .. }) = &*call.func else {
        return;
    };

    // Ensure the method called is `update`
    if attr != "update" {
        return;
    }

    // Must have exactly one positional argument and no keyword arguments
    if call.arguments.args.len() != 1 || !call.arguments.keywords.is_empty() {
        return;
    }

    // The argument must be a dictionary literal with exactly one key-value pair
    let Some(Expr::Dict(dict_literal)) = call.arguments.args.first() else {
        return;
    };

    if dict_literal.items.len() != 1 {
        return;
    }

    let item = &dict_literal.items[0];

    // Must be a regular key-value pair, not a dictionary unpacking (`**`)
    let Some(key) = &item.key else {
        return;
    };

    // Check if the object is a known dict
    let Some(name) = value.as_name_expr() else {
        return;
    };

    if !typing::is_known_to_be_of_type_dict(checker.semantic(), name) {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(DictUpdateWithSingleItem, call.range());

    // Only provide a fix when the call is a standalone expression statement
    let stmt = checker.semantic().current_statement();
    let ast::Stmt::Expr(expr_stmt) = stmt else {
        return;
    };

    // Ensure the expression statement's value is exactly this call
    if expr_stmt.value.range() != call.range() {
        return;
    }

    let locator = checker.locator();
    let dict_name = locator.slice(value.range());
    let key_source = locator.slice(key.range());
    let value_source = locator.slice(item.value.range());

    let replacement = format!("{dict_name}[{key_source}] = {value_source}");
    let edit = Edit::range_replacement(replacement, stmt.range());

    diagnostic.set_fix(Fix::unsafe_edit(edit));
}
