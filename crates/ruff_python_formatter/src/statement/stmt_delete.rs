use ruff_formatter::write;
use ruff_python_ast::StmtDelete;
use ruff_text_size::Ranged;

use crate::builders::{parenthesize_if_expands, PyFormatterExtensions};
use crate::comments::{dangling_node_comments, SourceComment};
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::{has_skip_comment, prelude::*};

#[derive(Default)]
pub struct FormatStmtDelete;

impl FormatNodeRule<StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, item: &StmtDelete, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtDelete { range: _, targets } = item;

        write!(f, [token("del"), space()])?;

        match targets.as_slice() {
            [] => {
                write!(
                    f,
                    [
                        // Handle special case of delete statements without targets.
                        // ```
                        // del (
                        //     # Dangling comment
                        // )
                        token("("),
                        block_indent(&dangling_node_comments(item)),
                        token(")"),
                    ]
                )
            }
            [single] => {
                write!(
                    f,
                    [maybe_parenthesize_expression(
                        single,
                        item,
                        Parenthesize::IfBreaks
                    )]
                )
            }
            targets => {
                let item = format_with(|f| {
                    f.join_comma_separated(item.end())
                        .nodes(targets.iter())
                        .finish()
                });
                parenthesize_if_expands(&item).fmt(f)
            }
        }
    }

    fn is_suppressed(
        &self,
        trailing_comments: &[SourceComment],
        context: &PyFormatContext,
    ) -> bool {
        has_skip_comment(trailing_comments, context.source())
    }
}
