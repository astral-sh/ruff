use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, comparable::ComparableExpr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for redundant `Final[Literal[]]` annotations.
///
/// ## Why is this bad?
/// A `Final[Literal[x]]` annotation can be replaced with just `Final`.
///
/// ## Example
///
/// ```python
/// x: Final[Literal[42]]
/// # or,
/// x: Final[Literal[42]] = 42
/// ```
///
/// Use instead:
/// ```python
/// x: Final = 42
/// ```
#[violation]
pub struct RedundantFinalLiteral {
    literal: SourceCodeSnippet,
}

impl Violation for RedundantFinalLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantFinalLiteral { literal } = self;
        if let Some(literal) = literal.full_display() {
            format!("`Final[Literal[{literal}]]` can be replaced with a bare `Final`")
        } else {
            format!("`Final[Literal[...]] can be replaced with a bare `Final`")
        }
    }

    fn fix_title(&self) -> Option<String> {
        let RedundantFinalLiteral { literal } = self;
        if let Some(literal) = literal.full_display() {
            Some(format!(
                "Replace `Final[Literal[{literal}]]` with a bare `Final`"
            ))
        } else {
            Some(format!("Replace `Final[Literal[...]] with a bare `Final`"))
        }
    }
}

/// PYI064
pub(crate) fn redundant_final_literal(checker: &mut Checker, ann_assign: &ast::StmtAnnAssign) {
    let ast::StmtAnnAssign {
        value: assign_value,
        annotation,
        ..
    } = ann_assign;

    let ast::Expr::Subscript(ast::ExprSubscript {
        slice: literal_slice,
        ..
    }) = &**annotation
    else {
        return;
    };
    let ast::Expr::Subscript(ast::ExprSubscript { slice: literal, .. }) = &**literal_slice else {
        return;
    };

    // If the Literal contains multiple elements, don't raise issue
    if let ast::Expr::Tuple(_) = &**literal {
        return;
    }

    checker.diagnostics.push(
        Diagnostic::new(
            RedundantFinalLiteral {
                literal: SourceCodeSnippet::from_str(checker.locator().slice(literal.range())),
            },
            ann_assign.range(),
        )
        .with_fix(generate_fix(
            checker,
            annotation,
            assign_value.as_deref(),
            literal,
        )),
    );
}

fn generate_fix(
    checker: &Checker,
    annotation: &ast::Expr,
    assign_value: Option<&ast::Expr>,
    literal: &ast::Expr,
) -> Fix {
    let deletion = Edit::range_deletion(annotation.range());
    let insertion = Edit::insertion(format!("Final"), annotation.start());

    let Some(assign_value) = assign_value else {
        // If no assignment exists, add our own, same as the literal value.
        let literal_source = checker.locator().slice(literal.range());
        let assignment = Edit::insertion(format!(" = {literal_source}"), annotation.end());
        return Fix::safe_edits(deletion, [insertion, assignment]);
    };

    if ComparableExpr::from(assign_value) != ComparableExpr::from(literal) {
        // In this case, assume that the value in the literal annotation
        // is the correct one.
        let literal_source = checker.locator().slice(literal.range());
        let assign_replacement = Edit::replacement(
            literal_source.to_string(),
            assign_value.start(),
            assign_value.end(),
        );
        return Fix::unsafe_edits(deletion, [insertion, assign_replacement]);
    }

    Fix::safe_edits(deletion, [insertion])
}
