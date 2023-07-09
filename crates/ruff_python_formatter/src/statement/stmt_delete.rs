use crate::builders::{optional_parentheses, PyFormatterExtensions};
use crate::comments::{dangling_node_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::PyFormatContext;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{block_indent, space, text, Formatter};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::{Expr, StmtDelete};

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
                        // Handle special case of delete statements without elements.
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
            targets => optional_parentheses(&ExprSequence::new(targets)).fmt(f),
        }
    }
}

// TODO(cnpryer): Shared `ExprSequence` (see expr_tuple.rs)
#[derive(Debug)]
struct ExprSequence<'a> {
    targets: &'a [Expr],
}

impl<'a> ExprSequence<'a> {
    const fn new(targets: &'a [Expr]) -> Self {
        Self { targets }
    }
}

impl Format<PyFormatContext<'_>> for ExprSequence<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        f.join_comma_separated().nodes(self.targets.iter()).finish()
    }
}

impl NeedsParentheses for StmtDelete {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
