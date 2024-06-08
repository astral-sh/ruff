use ruff_formatter::write;
use ruff_python_ast::AstNode;
use ruff_python_ast::MatchCase;

use crate::builders::parenthesize_if_expands;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses, Parentheses};
use crate::prelude::*;
use crate::statement::clause::{clause_body, clause_header, ClauseHeader};

#[derive(Default)]
pub struct FormatMatchCase;

impl FormatNodeRule<MatchCase> for FormatMatchCase {
    fn fmt_fields(&self, item: &MatchCase, f: &mut PyFormatter) -> FormatResult<()> {
        let MatchCase {
            range: _,
            pattern,
            guard,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);

        write!(
            f,
            [
                clause_header(
                    ClauseHeader::MatchCase(item),
                    dangling_item_comments,
                    &format_with(|f| {
                        write!(f, [token("case"), space()])?;

                        let has_comments = comments.has_leading(pattern)
                            || comments.has_trailing_own_line(pattern);

                        if has_comments {
                            pattern.format().with_options(Parentheses::Always).fmt(f)?;
                        } else {
                            match pattern.needs_parentheses(item.as_any_node_ref(), f.context()) {
                                OptionalParentheses::Multiline => {
                                    parenthesize_if_expands(
                                        &pattern.format().with_options(Parentheses::Never),
                                    )
                                    .fmt(f)?;
                                }
                                OptionalParentheses::Always => {
                                    pattern.format().with_options(Parentheses::Always).fmt(f)?;
                                }
                                OptionalParentheses::Never => {
                                    pattern.format().with_options(Parentheses::Never).fmt(f)?;
                                }
                                OptionalParentheses::BestFit => {
                                    pattern.format().with_options(Parentheses::Never).fmt(f)?;
                                }
                            }
                        }

                        if let Some(guard) = guard {
                            write!(f, [space(), token("if"), space(), guard.format()])?;
                        }

                        Ok(())
                    }),
                ),
                clause_body(body, dangling_item_comments),
            ]
        )
    }
}
