use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::{self as ast, CmpOp, Expr, ExprKind, Stmt, StmtIf};
use ruff_python_semantic::analyze::typing::{is_sys_version_block, is_type_checking_block};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for three or more consecutive if-statements with direct returns
///
/// ## Why is this bad?
/// These can be simplified by using a dictionary
///
/// ## Example
/// ```python
/// def find_phrase(x):
///     if x == 1:
///         return "Hello"
///     elif x == 2:
///         return "Goodbye"
///     elif x == 3:
///         return "Good morning"
///     else:
///         return "Goodnight"
/// ```
///
/// Use instead:
/// ```python
/// def find_phrase(x):
///     phrases = {1: "Hello", 2: "Goodbye", 3: "Good morning"}
///     return phrases.get(x, "Goodnight")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.250", category = Category::Pedantic)]
pub(crate) struct IfElseBlockInsteadOfDictLookup;

impl Violation for IfElseBlockInsteadOfDictLookup {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use a dictionary instead of consecutive `if` statements".to_string()
    }
}
/// SIM116
pub(crate) fn if_else_block_instead_of_dict_lookup(checker: &Checker, stmt_if: StmtIf<'_>) {
    // Throughout this rule:
    // * Each if or elif statement's test must consist of a constant equality check with the same variable.
    // * Each if or elif statement's body must consist of a single `return`.
    // * The else clause must be empty, or a single `return`.
    let body = stmt_if.body();
    let test = stmt_if.test();
    let elif_else_clauses = stmt_if.elif_else_clauses();

    let ExprKind::Compare(compare) = test.classify() else {
        return;
    };
    let Some((left, CmpOp::Eq, expr)) = compare.as_single() else {
        return;
    };
    let ExprKind::Name(name) = left.classify() else {
        return;
    };
    let target = name.id();
    let Some(literal_expr) = expr.as_literal_expr() else {
        return;
    };
    let Some([ast::StmtFields::Return(ast::StmtReturnFields { value, .. })]) =
        body.as_array().map(|items| items.map(Stmt::fields))
    else {
        return;
    };

    if value.is_some_and(|value| {
        contains_effect(value, |id| checker.semantic().has_builtin_binding(id))
    }) {
        return;
    }

    // Avoid suggesting ternary for `if sys.version_info >= ...`-style checks.
    if is_sys_version_block(stmt_if, checker.semantic()) {
        return;
    }

    // Avoid suggesting ternary for `if TYPE_CHECKING:`-style checks.
    if is_type_checking_block(stmt_if, checker.semantic()) {
        return;
    }

    // The `expr` was checked to be a literal above, so this is safe.
    let mut literals: FxHashSet<ComparableExpr> = FxHashSet::default();
    literals.insert(literal_expr.view().into());

    for clause in elif_else_clauses {
        let test = clause.test();
        let body = clause.body();
        let Some([ast::StmtFields::Return(ast::StmtReturnFields { value, .. })]) =
            body.as_array().map(|items| items.map(Stmt::fields))
        else {
            return;
        };

        match test.map(Expr::classify) {
            // `else`
            None => {
                // The else must also be a single effect-free return statement
                let Some([ast::StmtFields::Return(ast::StmtReturnFields { value, .. })]) =
                    body.as_array().map(|items| items.map(Stmt::fields))
                else {
                    return;
                };
                if value.is_some_and(|value| {
                    contains_effect(value, |id| checker.semantic().has_builtin_binding(id))
                }) {
                    return;
                }
            }
            // `elif`
            Some(ExprKind::Compare(compare)) => {
                let Some((left, CmpOp::Eq, expr)) = compare.as_single() else {
                    return;
                };
                let ExprKind::Name(name) = left.classify() else {
                    return;
                };
                let id = name.id();
                if id != target {
                    return;
                }
                let Some(literal_expr) = expr.as_literal_expr() else {
                    return;
                };

                if value.is_some_and(|value| {
                    contains_effect(value, |id| checker.semantic().has_builtin_binding(id))
                }) {
                    return;
                }

                // The `expr` was checked to be a literal above, so this is safe.
                literals.insert(literal_expr.view().into());
            }
            // Different `elif`
            _ => {
                return;
            }
        }
    }

    if literals.len() < 3 {
        return;
    }

    checker.report_diagnostic(IfElseBlockInsteadOfDictLookup, stmt_if.range());
}
