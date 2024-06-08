use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

use super::helpers;

/// ## What it does
/// Checks for `tuple` calls that take unnecessary list or tuple literals as
/// arguments.
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
/// ```
///
/// Use instead:
/// ```python
/// (1, 2)
/// (1, 2)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
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
pub(crate) fn unnecessary_literal_within_tuple_call(checker: &mut Checker, call: &ast::ExprCall) {
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
        _ => return,
    };

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinTupleCall {
            literal: argument_kind.to_string(),
        },
        call.range(),
    );

    // Convert `tuple([1, 2])` to `(1, 2)`
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

    checker.diagnostics.push(diagnostic);
}
