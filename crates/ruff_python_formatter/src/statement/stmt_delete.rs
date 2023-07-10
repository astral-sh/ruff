use crate::builders::{optional_parentheses, PyFormatterExtensions};
use crate::comments::dangling_node_comments;
use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{block_indent, format_with, space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::StmtDelete;

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
            // TODO(cnpryer): single and multiple targets should be handled the same since
            //   tuples require special formatting whereas this is just a sequence of expressions (tuple-like).
            [single] => {
                write!(f, [single.format().with_options(Parenthesize::IfBreaks)])
            }
            targets => {
                let item = format_with(|f| f.join_comma_separated().nodes(targets.iter()).finish());
                optional_parentheses(&item).fmt(f)
            }
        }
    }

    fn fmt_dangling_comments(&self, _node: &StmtDelete, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
