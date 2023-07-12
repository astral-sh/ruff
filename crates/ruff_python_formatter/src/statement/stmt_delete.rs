use crate::builders::{parenthesize_if_expands, PyFormatterExtensions};
use crate::comments::dangling_node_comments;
use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{block_indent, format_with, space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::{Ranged, StmtDelete};

#[derive(Default)]
pub struct FormatStmtDelete;

impl FormatNodeRule<StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, item: &StmtDelete, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtDelete { range: _, targets } = item;

        write!(f, [text("del"), space()])?;

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
                        &text("("),
                        block_indent(&dangling_node_comments(item)),
                        &text(")"),
                    ]
                )
            }
            [single] => {
                write!(f, [single.format().with_options(Parenthesize::IfBreaks)])
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

    fn fmt_dangling_comments(&self, _node: &StmtDelete, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
