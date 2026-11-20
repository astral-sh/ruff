use ruff_formatter::write;
use ruff_python_ast::StmtDelete;
use ruff_text_size::Ranged;

use crate::builders::{PyFormatterExtensions, parenthesize_if_expands};
use crate::comments::dangling_node_comments;
use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatStmtDelete;

impl FormatNodeRule<ruff_python_ast::kind::StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, item: StmtDelete<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let targets = item.targets();

        write!(f, [token("del"), space()])?;

        match targets {
            args if args.is_empty() => {
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
            args if let Some([single]) = args.as_array() => {
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
}
