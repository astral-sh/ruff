use rustc_hash::FxHashSet;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Expr, ExprCall, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for enums that contain duplicate values.
///
/// ## Why is this bad?
/// Enum values should be unique. Non-unique values are redundant and likely a
/// mistake.
///
/// ## Example
/// ```python
/// from enum import Enum
///
///
/// class Foo(Enum):
///     A = 1
///     B = 2
///     C = 1
/// ```
///
/// Use instead:
/// ```python
/// from enum import Enum
///
///
/// class Foo(Enum):
///     A = 1
///     B = 2
///     C = 3
/// ```
///
/// ## References
/// - [Python documentation: `enum.Enum`](https://docs.python.org/3/library/enum.html#enum.Enum)
#[derive(ViolationMetadata)]
pub(crate) struct NonUniqueEnums {
    value: String,
}

impl Violation for NonUniqueEnums {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonUniqueEnums { value } = self;
        format!("Enum contains duplicate value: `{value}`")
    }
}

/// PIE796
pub(crate) fn non_unique_enums(checker: &Checker, parent: &Stmt, body: &[Stmt]) {
    let semantic = checker.semantic();

    let Stmt::ClassDef(parent) = parent else {
        return;
    };

    if !parent.bases().iter().any(|expr| {
        semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["enum", "Enum"]))
    }) {
        return;
    }

    let mut seen_targets: FxHashSet<ComparableExpr> = FxHashSet::default();
    for stmt in body {
        let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
            continue;
        };

        if let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() {
            if checker
                .semantic()
                .resolve_qualified_name(func)
                .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["enum", "auto"]))
            {
                continue;
            }
        }

        if checker.source_type.is_stub() && member_has_unknown_value(checker, value) {
            continue;
        }

        let comparable = ComparableExpr::from(value);

        if !seen_targets.insert(comparable) {
            let diagnostic = Diagnostic::new(
                NonUniqueEnums {
                    value: checker.generator().expr(value),
                },
                stmt.range(),
            );
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Whether the value is a bare ellipsis literal (`A = ...`)
/// or a casted one (`A = cast(SomeType, ...)`).
fn member_has_unknown_value(checker: &Checker, expr: &Expr) -> bool {
    match expr {
        Expr::EllipsisLiteral(_) => true,

        Expr::Call(ExprCall {
            func, arguments, ..
        }) => {
            if !checker.semantic().match_typing_expr(func, "cast") {
                return false;
            }

            if !arguments.keywords.is_empty() {
                return false;
            }

            matches!(arguments.args.as_ref(), [_, Expr::EllipsisLiteral(_)])
        }

        _ => false,
    }
}
