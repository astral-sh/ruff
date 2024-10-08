use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, comparable::ComparableExpr};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for redundant `Final[Literal[...]]` annotations.
///
/// ## Why is this bad?
/// A `Final[Literal[...]]` annotation can be replaced with `Final`; the literal
/// use is unnecessary.
///
/// ## Example
///
/// ```pyi
/// x: Final[Literal[42]]
/// y: Final[Literal[42]] = 42
/// ```
///
/// Use instead:
/// ```pyi
/// x: Final = 42
/// y: Final = 42
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
        format!(
            "`Final[Literal[{literal}]]` can be replaced with a bare `Final`",
            literal = literal.truncated_display()
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `Final`".to_string())
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

    let ast::Expr::Subscript(annotation) = &**annotation else {
        return;
    };

    // Ensure it is `Final[Literal[...]]`.
    let ast::Expr::Subscript(ast::ExprSubscript {
        value,
        slice: literal,
        ..
    }) = &*annotation.slice
    else {
        return;
    };
    if !checker.semantic().match_typing_expr(value, "Literal") {
        return;
    }

    // Discards tuples like `Literal[1, 2, 3]` and complex literals like `Literal[{1, 2}]`.
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

    // The literal value and the assignment value being different doesn't make sense, so we skip
    // fixing in that case.
    if let Some(assign_value) = assign_value.as_ref() {
        if ComparableExpr::from(assign_value) == ComparableExpr::from(literal) {
            diagnostic.set_fix(generate_fix(annotation, None, checker.locator()));
        }
    } else {
        diagnostic.set_fix(generate_fix(annotation, Some(literal), checker.locator()));
    }

    checker.diagnostics.push(diagnostic);
}

/// Generate a fix to convert a `Final[Literal[...]]` annotation to a `Final` annotation.
fn generate_fix(
    annotation: &ast::ExprSubscript,
    literal: Option<&ast::Expr>,
    locator: &Locator,
) -> Fix {
    // Remove the `Literal[...]` part from `Final[Literal[...]]`.
    let deletion = Edit::range_deletion(
        annotation
            .slice
            .range()
            .sub_start(TextSize::new(1))
            .add_end(TextSize::new(1)),
    );

    // If a literal was provided, insert an assignment.
    //
    // For example, change `x: Final[Literal[42]]` to `x: Final = 42`.
    if let Some(literal) = literal {
        let assignment = Edit::insertion(
            format!(
                " = {literal_source}",
                literal_source = locator.slice(literal)
            ),
            annotation.end(),
        );
        Fix::safe_edits(deletion, std::iter::once(assignment))
    } else {
        Fix::safe_edit(deletion)
    }
}
