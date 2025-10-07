use ruff_formatter::write;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::ExprDictComp;
use ruff_text_size::Ranged;

use crate::comments::dangling_comments;
use crate::expression::comprehension_helpers::is_comprehension_multiline;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::options::ComprehensionLineBreak;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprDictComp;

impl FormatNodeRule<ExprDictComp> for FormatExprDictComp {
    fn fmt_fields(&self, item: &ExprDictComp, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprDictComp {
            range: _,
            node_index: _,
            key,
            value,
            generators,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        // Dangling comments can either appear after the open bracket, or around the key-value
        // pairs:
        // ```python
        // {  # open_parenthesis_comments
        //     x:  # key_value_comments
        //     y
        //     for (x, y) in z
        // }
        // ```
        let (open_parenthesis_comments, key_value_comments) =
            dangling.split_at(dangling.partition_point(|comment| comment.end() < key.start()));

        // Check if we should preserve multi-line formatting
        let should_preserve_multiline =
            f.options().comprehension_line_break() == ComprehensionLineBreak::Preserve
            && is_comprehension_multiline(item, f.context());

        let formatted_content = format_with(|f| {
            write!(f, [group(&key.format()), token(":")])?;

            if key_value_comments.is_empty() {
                space().fmt(f)?;
            } else {
                dangling_comments(key_value_comments).fmt(f)?;
            }

            write!(f, [value.format(), soft_line_break_or_space()])?;

            f.join_with(soft_line_break_or_space())
                .entries(generators.iter().formatted())
                .finish()
        });

        write!(
            f,
            [parenthesized(
                "{",
                &if should_preserve_multiline {
                    // Force expansion to preserve multi-line format
                    group(&formatted_content).should_expand(true)
                } else {
                    // Default behavior - try to fit on one line
                    group(&formatted_content)
                },
                "}"
            )
            .with_dangling_comments(open_parenthesis_comments)]
        )
    }
}

impl NeedsParentheses for ExprDictComp {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
