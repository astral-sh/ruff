use crate::checkers::ast::Checker;
use ruff_python_ast::Expr;

/// Returns true if the function call is ngettext
pub(crate) fn is_ngettext_call(checker: &Checker, func: &Expr) -> bool {
    let semantic = checker.semantic();

    // Check if it's a direct name reference to ngettext
    if let Some(name) = func.as_name_expr() {
        if name.id == "ngettext" {
            return true;
        }
    }

    // Check if it's a qualified name ending with ngettext
    if let Some(qualified_name) = semantic.resolve_qualified_name(func) {
        return matches!(qualified_name.segments(), [.., "ngettext"]);
    }

    false
}
