use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for ternary `if` expressions that can be replaced with
/// `or` expressions.
///
/// ## Why is this bad?
/// Ternary if statements are more verbose than `or` expressions, and
/// generally have the same functionality.
///
/// ## Example
/// ```python
/// z = x if x else y
/// ```
///
/// Use instead:
/// ```python
/// z = x or y
/// ```
///
/// Note: if `x` depends on side-effects, then this check should be ignored.
#[violation]
pub struct OrOper {
    if_true: SourceCodeSnippet,
    if_false: SourceCodeSnippet,
}

impl Violation for OrOper {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let OrOper { if_true, if_false } = self;

        match (if_true.full_display(), if_false.full_display()) {
            (_, None) | (None, _) => {
                format!("Replace ternary `if` expression with `or` expression")
            }
            (Some(if_true), Some(if_false)) => {
                format!(
                    "Replace `{if_true} if {if_true} or {if_false}` with `{if_true} or {if_false}`"
                )
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Use `or` expression"))
    }
}

/// FURB110
pub(crate) fn or_oper(checker: &mut Checker, if_expr: &ast::ExprIf) {
    let ast::ExprIf {
        test,
        body,
        orelse,
        range,
    } = if_expr;

    let if_true = body.as_ref();
    let if_true_code = checker.locator().slice(if_true);
    let test_expr_code = checker.locator().slice(test.as_ref());

    if test_expr_code != if_true_code {
        return;
    }

    let if_false = orelse.as_ref();

    let mut diagnostic = Diagnostic::new(
        OrOper {
            if_true: SourceCodeSnippet::from_str(if_true_code),
            if_false: SourceCodeSnippet::from_str(checker.locator().slice(if_false)),
        },
        *range,
    );

    let mut tokenizer = SimpleTokenizer::starts_at(if_true.end(), checker.locator().contents());

    // find the `else` token to replace with `or`
    let else_token = tokenizer
        .find(|tok| tok.kind() == SimpleTokenKind::Else)
        .expect("else token to exist");

    let fix = Fix::unsafe_edits(
        Edit::range_replacement("or".to_string(), else_token.range()),
        [Edit::deletion(if_true.start(), test.start())],
    );

    diagnostic.set_fix(fix);

    checker.diagnostics.push(diagnostic);
}
