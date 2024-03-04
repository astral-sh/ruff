use rustc_hash::FxHashSet;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{self as ast, Expr, PySourceType, Stmt};
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
#[violation]
pub struct NonUniqueEnums {
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
pub(crate) fn non_unique_enums(checker: &mut Checker, parent: &Stmt, body: &[Stmt]) {
    let Stmt::ClassDef(parent) = parent else {
        return;
    };

    if !parent.bases().iter().any(|expr| {
        checker
            .semantic()
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

        let comparable = ComparableExpr::from(value);

        if checker.source_type == PySourceType::Stub
            && comparable == ComparableExpr::EllipsisLiteral
        {
            continue;
        }

        if !seen_targets.insert(comparable) {
            let diagnostic = Diagnostic::new(
                NonUniqueEnums {
                    value: checker.generator().expr(value),
                },
                stmt.range(),
            );
            checker.diagnostics.push(diagnostic);
        }
    }
}
