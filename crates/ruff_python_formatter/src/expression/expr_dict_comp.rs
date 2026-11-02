use ruff_formatter::write;
use ruff_python_ast::{ExprDictComp, SyntaxNode};
use ruff_text_size::Ranged;

use crate::comments::dangling_comments;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, parenthesized};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprDictComp;

impl FormatNodeRule<ruff_python_ast::kind::ExprDictComp> for FormatExprDictComp {
    fn fmt_fields(&self, item: ExprDictComp<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let key = item.key();
        let value = item.value();
        let generators = item.generators();

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
        let first_expression = key.unwrap_or(value);
        let (open_parenthesis_comments, key_value_comments) = dangling
            .split_at(dangling.partition_point(|comment| comment.end() < first_expression.start()));

        write!(
            f,
            [parenthesized(
                "{",
                &group(&format_with(|f| {
                    if let Some(key) = key {
                        write!(f, [group(&key.format()), token(":")])?;

                        if key_value_comments.is_empty() {
                            space().fmt(f)?;
                        } else {
                            dangling_comments(key_value_comments).fmt(f)?;
                        }
                    } else {
                        write!(f, [token("**")])?;
                        if let Some(first) = comments.leading(value).first() {
                            if first.line_position().is_own_line() {
                                hard_line_break().fmt(f)?;
                            } else {
                                write!(f, [space(), space()])?;
                            }
                        }
                    }

                    write!(f, [value.format(), soft_line_break_or_space()])?;

                    f.join_with(soft_line_break_or_space())
                        .entries(generators.iter().formatted())
                        .finish()
                })),
                "}"
            )
            .with_dangling_comments(open_parenthesis_comments)]
        )
    }
}

impl NeedsParentheses for ExprDictComp<'_> {
    fn needs_parentheses(
        &self,
        _parent: SyntaxNode,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
