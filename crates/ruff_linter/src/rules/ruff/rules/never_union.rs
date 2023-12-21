use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `typing.NoReturn` and `typing.Never` in union types.
///
/// ## Why is this bad?
/// `typing.NoReturn` and `typing.Never` are special types, used to indicate
/// that a function never returns, or that a type has no values.
///
/// Including `typing.NoReturn` or `typing.Never` in a union type is redundant,
/// as, e.g., `typing.Never | T` is equivalent to `T`.
///
/// ## Example
/// ```python
/// from typing import Never
///
///
/// def func() -> Never | int:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def func() -> int:
///     ...
/// ```
///
/// ## Options
/// - [Python documentation: `typing.Never`](https://docs.python.org/3/library/typing.html#typing.Never)
/// - [Python documentation: `typing.NoReturn`](https://docs.python.org/3/library/typing.html#typing.NoReturn)
#[violation]
pub struct NeverUnion {
    never_like: NeverLike,
    union_like: UnionLike,
}

impl Violation for NeverUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            never_like,
            union_like,
        } = self;
        match union_like {
            UnionLike::BinOp => {
                format!("`{never_like} | T` is equivalent to `T`")
            }
            UnionLike::TypingUnion => {
                format!("`Union[{never_like}, T]` is equivalent to `T`")
            }
        }
    }
}

/// RUF020
pub(crate) fn never_union<'a>(checker: &mut Checker, expr: &'a Expr) {
    let mut expressions: Vec<(NeverLike, UnionLike, &'a Expr)> = Vec::new();
    let mut rest: Vec<&'a Expr> = Vec::new();

    // Find all `typing.Never` and `typing.NoReturn` expressions.
    let semantic = checker.semantic();
    let mut collect_never = |expr: &'a Expr, parent: Option<&'a Expr>| {
        if let Some(call_path) = semantic.resolve_call_path(expr) {
            let union_like = if parent.is_some_and(Expr::is_bin_op_expr) {
                UnionLike::BinOp
            } else {
                UnionLike::TypingUnion
            };

            let never_like = if semantic.match_typing_call_path(&call_path, "NoReturn") {
                Some(NeverLike::NoReturn)
            } else if semantic.match_typing_call_path(&call_path, "Never") {
                Some(NeverLike::Never)
            } else {
                None
            };

            if let Some(never_like) = never_like {
                expressions.push((never_like, union_like, expr));
                return;
            }
        }

        rest.push(expr);
    };

    traverse_union(&mut collect_never, checker.semantic(), expr, None);

    // Create a diagnostic for each `typing.Never` and `typing.NoReturn` expression.
    for (never_like, union_like, child) in expressions {
        let diagnostic = Diagnostic::new(
            NeverUnion {
                never_like,
                union_like,
            },
            child.range(),
        );
        checker.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnionLike {
    /// E.g., `typing.Union[int, str]`
    TypingUnion,
    /// E.g., `int | str`
    BinOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NeverLike {
    /// E.g., `typing.NoReturn`
    NoReturn,
    /// E.g., `typing.Never`
    Never,
}

impl std::fmt::Display for NeverLike {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeverLike::NoReturn => f.write_str("NoReturn"),
            NeverLike::Never => f.write_str("Never"),
        }
    }
}
