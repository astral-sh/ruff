use ruff_formatter::{FormatResult, write};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprListComp;

use crate::expression::comprehension_helpers::is_comprehension_multiline;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::options::ComprehensionLineBreak;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprListComp;

impl FormatNodeRule<ExprListComp> for FormatExprListComp {
    fn fmt_fields(&self, item: &ExprListComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprListComp {
            range: _,
            node_index: _,
            elt,
            generators,
        } = item;

        let joined = format_with(|f| {
            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        // Check if we should preserve multi-line formatting
        let should_preserve_multiline =
            f.options().comprehension_line_break() == ComprehensionLineBreak::Preserve
            && is_comprehension_multiline(item, f.context());

        let formatted_content = format_with(|f| {
            write!(f, [
                group(&elt.format()),
                soft_line_break_or_space(),
                joined
            ])
        });

        write!(
            f,
            [parenthesized(
                "[",
                &if should_preserve_multiline {
                    // Force expansion to preserve multi-line format
                    group(&formatted_content).should_expand(true)
                } else {
                    // Default behavior - try to fit on one line
                    group(&formatted_content)
                },
                "]"
            )
            .with_dangling_comments(dangling)]
        )
    }
}

impl NeedsParentheses for ExprListComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
