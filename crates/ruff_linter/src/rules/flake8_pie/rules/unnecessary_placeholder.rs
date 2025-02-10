use ruff_diagnostics::{AlwaysFixableViolation, Applicability};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::whitespace::trailing_comment_start_offset;
use ruff_python_ast::{Expr, ExprStringLiteral, Stmt, StmtExpr};
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for unnecessary `pass` statements and ellipsis (`...`) literals in
/// functions, classes, and other blocks.
///
/// ## Why is this bad?
/// In Python, the `pass` statement and ellipsis (`...`) literal serve as
/// placeholders, allowing for syntactically correct empty code blocks. The
/// primary purpose of these nodes is to avoid syntax errors in situations
/// where a statement or expression is syntactically required, but no code
/// needs to be executed.
///
/// If a `pass` or ellipsis is present in a code block that includes at least
/// one other statement (even, e.g., a docstring), it is unnecessary and should
/// be removed.
///
/// ## Example
/// ```python
/// def func():
///     """Placeholder docstring."""
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     """Placeholder docstring."""
/// ```
///
/// Or, given:
/// ```python
/// def func():
///     """Placeholder docstring."""
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     """Placeholder docstring."""
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe in the rare case that the `pass` or ellipsis
/// is followed by a string literal, since removal of the placeholder would convert the
/// subsequent string literal into a docstring.
///
/// ## References
/// - [Python documentation: The `pass` statement](https://docs.python.org/3/reference/simple_stmts.html#the-pass-statement)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryPlaceholder {
    kind: Placeholder,
}

impl AlwaysFixableViolation for UnnecessaryPlaceholder {
    #[derive_message_formats]
    fn message(&self) -> String {
        match &self.kind {
            Placeholder::Pass => "Unnecessary `pass` statement".to_string(),
            Placeholder::Ellipsis => "Unnecessary `...` literal".to_string(),
        }
    }

    fn fix_title(&self) -> String {
        let title = match &self.kind {
            Placeholder::Pass => "Remove unnecessary `pass`",
            Placeholder::Ellipsis => "Remove unnecessary `...`",
        };
        title.to_string()
    }
}

/// PIE790
pub(crate) fn unnecessary_placeholder(checker: &Checker, body: &[Stmt]) {
    if body.len() < 2 {
        return;
    }

    for (index, stmt) in body.iter().enumerate() {
        let kind = match stmt {
            Stmt::Pass(_) => Placeholder::Pass,
            Stmt::Expr(expr) if expr.value.is_ellipsis_literal_expr() => {
                // In a type-checking block, a trailing ellipsis might be meaningful.
                // A user might be using the type-checking context to declare a stub.
                if checker.semantic().in_type_checking_block() {
                    return;
                }

                // Ellipses are significant in protocol methods and abstract methods.
                // Specifically, Pyright uses the presence of an ellipsis to indicate that
                // a method is a stub, rather than a default implementation.
                if in_protocol_or_abstract_method(checker.semantic()) {
                    return;
                }
                Placeholder::Ellipsis
            }
            _ => continue,
        };

        let next_stmt = body.get(index + 1);
        add_diagnostic(checker, stmt, next_stmt, kind);
    }
}

/// Add a diagnostic for the given statement.
fn add_diagnostic(
    checker: &Checker,
    stmt: &Stmt,
    next_stmt: Option<&Stmt>,
    placeholder_kind: Placeholder,
) {
    let edit = if let Some(index) = trailing_comment_start_offset(stmt, checker.source()) {
        Edit::range_deletion(stmt.range().add_end(index))
    } else {
        fix::edits::delete_stmt(stmt, None, checker.locator(), checker.indexer())
    };
    let applicability = match next_stmt {
        // Mark the fix as unsafe if the following statement is a string literal,
        // as it will become the module/class/function's docstring after the fix.
        Some(Stmt::Expr(StmtExpr { value, .. })) => match value.as_ref() {
            Expr::StringLiteral(ExprStringLiteral { .. }) => Applicability::Unsafe,
            _ => Applicability::Safe,
        },
        _ => Applicability::Safe,
    };

    let isolation_level = Checker::isolation(checker.semantic().current_statement_id());
    let fix = Fix::applicable_edit(edit, applicability).isolate(isolation_level);

    let diagnostic = Diagnostic::new(
        UnnecessaryPlaceholder {
            kind: placeholder_kind,
        },
        stmt.range(),
    );

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

#[derive(Debug, PartialEq, Eq)]
enum Placeholder {
    Pass,
    Ellipsis,
}

impl std::fmt::Display for Placeholder {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Pass => fmt.write_str("pass"),
            Self::Ellipsis => fmt.write_str("..."),
        }
    }
}

/// Return `true` if the [`SemanticModel`] is in a `typing.Protocol` subclass or an abstract
/// method.
fn in_protocol_or_abstract_method(semantic: &SemanticModel) -> bool {
    semantic.current_scopes().any(|scope| match scope.kind {
        ScopeKind::Class(class_def) => class_def
            .bases()
            .iter()
            .any(|base| semantic.match_typing_expr(map_subscript(base), "Protocol")),
        ScopeKind::Function(function_def) => {
            ruff_python_semantic::analyze::visibility::is_abstract(
                &function_def.decorator_list,
                semantic,
            )
        }
        _ => false,
    })
}
