use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for `tuple` calls that take unnecessary list or tuple literals as
/// arguments. In [preview], this also includes unnecessary list comprehensions
/// within tuple calls.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a `tuple()` call,
/// since there is a literal syntax for these types.
///
/// If a list literal was passed, then it should be rewritten as a `tuple`
/// literal. Otherwise, if a tuple literal was passed, then the outer call
/// to `tuple()` should be removed.
///
/// In [preview], this rule also checks for list comprehensions within `tuple()`
/// calls. If a list comprehension is found, it should be rewritten as a
/// generator expression.
///
/// ## Example
/// ```python
/// tuple([1, 2])
/// tuple((1, 2))
/// tuple([x for x in range(10)])
/// ```
///
/// Use instead:
/// ```python
/// (1, 2)
/// (1, 2)
/// tuple(x for x in range(10))
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryLiteralWithinTupleCall {
    literal_kind: TupleLiteralKind,
}

impl AlwaysFixableViolation for UnnecessaryLiteralWithinTupleCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.literal_kind {
            TupleLiteralKind::List => {
                "Unnecessary list literal passed to `tuple()` (rewrite as a tuple literal)"
                    .to_string()
            }
            TupleLiteralKind::Tuple => {
                "Unnecessary tuple literal passed to `tuple()` (remove the outer call to `tuple()`)"
                    .to_string()
            }
            TupleLiteralKind::ListComp => {
                "Unnecessary list comprehension passed to `tuple()` (rewrite as a generator)"
                    .to_string()
            }
        }
    }

    fn fix_title(&self) -> String {
        let title = match self.literal_kind {
            TupleLiteralKind::List => "Rewrite as a tuple literal",
            TupleLiteralKind::Tuple => "Remove the outer call to `tuple()`",
            TupleLiteralKind::ListComp => "Rewrite as a generator",
        };
        title.to_string()
    }
}

/// C409
pub(crate) fn unnecessary_literal_within_tuple_call(
    checker: &Checker,
    expr: &Expr,
    call: &ast::ExprCall,
) {
    if !call.arguments.keywords.is_empty() {
        return;
    }
    let Some(argument) = helpers::exactly_one_argument_with_matching_function(
        "tuple",
        &call.func,
        &call.arguments.args,
        &call.arguments.keywords,
    ) else {
        return;
    };
    let argument_kind = match argument {
        Expr::Tuple(_) => TupleLiteralKind::Tuple,
        Expr::List(_) => TupleLiteralKind::List,
        Expr::ListComp(_) if checker.settings.preview.is_enabled() => TupleLiteralKind::ListComp,
        _ => return,
    };
    if !checker.semantic().has_builtin_binding("tuple") {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinTupleCall {
            literal_kind: argument_kind,
        },
        call.range(),
    );

    match argument {
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            // Convert `tuple([1, 2])` to `(1, 2)`
            diagnostic.set_fix({
                let needs_trailing_comma = if let [item] = elts.as_slice() {
                    SimpleTokenizer::new(
                        checker.locator().contents(),
                        TextRange::new(item.end(), call.end()),
                    )
                    .all(|token| token.kind != SimpleTokenKind::Comma)
                } else {
                    false
                };

                // Replace `[` with `(`.
                let elt_start = Edit::replacement(
                    "(".into(),
                    call.start(),
                    argument.start() + TextSize::from(1),
                );
                // Replace `]` with `)` or `,)`.
                let elt_end = Edit::replacement(
                    if needs_trailing_comma {
                        ",)".into()
                    } else {
                        ")".into()
                    },
                    argument.end() - TextSize::from(1),
                    call.end(),
                );
                Fix::unsafe_edits(elt_start, [elt_end])
            });
        }

        Expr::ListComp(ast::ExprListComp { elt, .. }) => {
            if any_over_expr(elt, &Expr::is_await_expr) {
                return;
            }
            // Convert `tuple([x for x in range(10)])` to `tuple(x for x in range(10))`
            diagnostic.try_set_fix(|| {
                fixes::fix_unnecessary_comprehension_in_call(
                    expr,
                    checker.locator(),
                    checker.stylist(),
                )
            });
        }

        _ => return,
    }

    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, PartialEq, Eq)]
enum TupleLiteralKind {
    List,
    Tuple,
    ListComp,
}
