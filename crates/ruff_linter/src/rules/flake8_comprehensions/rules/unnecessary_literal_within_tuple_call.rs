use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for `tuple` calls that take unnecessary list or tuple literals as
/// arguments. This includes literals in the form of list or set comprehensions.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a `tuple()` call,
/// since there is a literal syntax for these types.
///
/// If a list literal was passed, then it should be rewritten as a `tuple`
/// literal. Otherwise, if a tuple literal was passed, then the outer call
/// to `tuple()` should be removed.
///
/// ## Examples
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
/// Moreover, in the case where a comprehension is replaced by a generator,
/// code behavior may be changed if the iteration has side-effects.
///
#[violation]
pub struct UnnecessaryLiteralWithinTupleCall {
    literal: String,
}

impl AlwaysFixableViolation for UnnecessaryLiteralWithinTupleCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall { literal } = self;
        if literal == "list" {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (rewrite as a `tuple` literal)"
            )
        } else {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (remove the outer call to `tuple()`)"
            )
        }
    }

    fn fix_title(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall { literal } = self;
        {
            if literal == "list" {
                "Rewrite as a `tuple` literal".to_string()
            } else {
                "Remove outer `tuple` call".to_string()
            }
        }
    }
}

/// C409
pub(crate) fn unnecessary_literal_within_tuple_call(
    checker: &mut Checker,
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
    if !checker.semantic().has_builtin_binding("tuple") {
        return;
    }
    let argument_kind = match argument {
        Expr::Tuple(_) => "tuple",
        Expr::List(_) => "list",
        Expr::ListComp(_) => "list comprehension",
        Expr::SetComp(_) => "set comprehension",
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinTupleCall {
            literal: argument_kind.to_string(),
        },
        call.range(),
    );

    // Convert `tuple([1, 2])` to `(1, 2)`
    if argument.is_tuple_expr() | argument.is_list_expr() {
        diagnostic.set_fix({
            let elts = match argument {
                Expr::List(ast::ExprList { elts, .. }) => elts.as_slice(),
                Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.as_slice(),
                _ => return,
            };

            let needs_trailing_comma = if let [item] = elts {
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
    } else if argument.is_list_comp_expr() | argument.is_set_comp_expr() {
        let (Expr::ListComp(ast::ExprListComp { elt, .. })
        | Expr::SetComp(ast::ExprSetComp { elt, .. })) = argument
        else {
            return;
        };
        if any_over_expr(elt, &Expr::is_await_expr) {
            return;
        }
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_comprehension_in_call(expr, checker.locator(), checker.stylist())
        });
    } else {
        return;
    }

    checker.diagnostics.push(diagnostic);
}
