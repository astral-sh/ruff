use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, comparable::ComparableExpr, helpers::map_subscript};
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
    if !checker.semantic().seen_typing() {
        return;
    }

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

    // Ensure it is `Final[Literal[...]]`
    if !checker
        .semantic()
        .match_typing_expr(map_subscript(literal_slice), "Literal")
    {
        return;
    }
    let ast::Expr::Subscript(ast::ExprSubscript { slice: literal, .. }) = &**literal_slice else {
        return;
    };

    // Discards tuples like `Literal[1, 2, 3]`
    // and complex literals like `Literal[{1, 2}]`
    if !matches!(
        &**literal,
        ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
    ) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        RedundantFinalLiteral {
            literal: SourceCodeSnippet::from_str(checker.locator().slice(literal.range())),
        },
        ann_assign.range(),
    );
    // The literal value and the assignment value being different doesn't
    // make sense, so we don't do an autofix if that happens.
    if !assign_value.as_ref().is_some_and(|assign_value| {
        ComparableExpr::from(assign_value) != ComparableExpr::from(literal)
    }) {
        diagnostic.set_fix(generate_fix(
            checker,
            annotation,
            literal,
            assign_value.is_none(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

fn generate_fix(
    checker: &Checker,
    annotation: &ast::Expr,
    literal: &ast::Expr,
    add_assignment: bool,
) -> Fix {
    let deletion = Edit::range_deletion(annotation.range());
    let mut insertions = vec![Edit::insertion(format!("Final"), annotation.start())];

    if add_assignment {
        // If no assignment exists, add our own, same as the literal value.
        let literal_source = checker.locator().slice(literal.range());
        let assignment = Edit::insertion(format!(" = {literal_source}"), annotation.end());
        insertions.push(assignment);
    };

    Fix::safe_edits(deletion, insertions)
}
