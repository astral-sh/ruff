use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;

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
pub(crate) fn non_unique_enums<'a, 'b>(
    checker: &mut Checker<'a>,
    parent: &'b Stmt,
    body: &'b [Stmt],
) where
    'b: 'a,
{
    let Stmt::ClassDef(ast::StmtClassDef { bases, .. }) = parent else {
        return;
    };

    if !bases.iter().any(|expr| {
        checker
            .semantic()
            .resolve_call_path(expr)
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["enum", "Enum"])
            })
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
                .resolve_call_path(func)
                .map_or(false, |call_path| {
                    matches!(call_path.as_slice(), ["enum", "auto"])
                })
            {
                continue;
            }
        }

        if !seen_targets.insert(ComparableExpr::from(value)) {
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
