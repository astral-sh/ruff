use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{CmpOp, Expr};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

pub(super) fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

pub(super) fn generate_comparison(
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
    parent: AnyNodeRef,
    comment_ranges: &CommentRanges,
    locator: &Locator,
) -> String {
    let start = left.start();
    let end = comparators.last().map_or_else(|| left.end(), Ranged::end);
    let mut contents = String::with_capacity(usize::from(end - start));

    // Add the left side of the comparison.
    contents.push_str(
        locator.slice(
            parenthesized_range(left.into(), parent, comment_ranges, locator.contents())
                .unwrap_or(left.range()),
        ),
    );

    for (op, comparator) in ops.iter().zip(comparators) {
        // Add the operator.
        contents.push_str(match op {
            CmpOp::Eq => " == ",
            CmpOp::NotEq => " != ",
            CmpOp::Lt => " < ",
            CmpOp::LtE => " <= ",
            CmpOp::Gt => " > ",
            CmpOp::GtE => " >= ",
            CmpOp::In => " in ",
            CmpOp::NotIn => " not in ",
            CmpOp::Is => " is ",
            CmpOp::IsNot => " is not ",
        });

        // Add the right side of the comparison.
        contents.push_str(
            locator.slice(
                parenthesized_range(
                    comparator.into(),
                    parent,
                    comment_ranges,
                    locator.contents(),
                )
                .unwrap_or(comparator.range()),
            ),
        );
    }

    contents
}
