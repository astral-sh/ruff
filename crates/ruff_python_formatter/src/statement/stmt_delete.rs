use crate::prelude::PyFormatContext;
use crate::{expression::parentheses::Parenthesize, AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{format_with, space, text, Formatter};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::{Expr, StmtDelete};

#[derive(Default)]
pub struct FormatStmtDelete;

impl FormatNodeRule<StmtDelete> for FormatStmtDelete {
    fn fmt_fields(&self, item: &StmtDelete, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtDelete { range: _, targets } = item;
        write!(f, [text("del"), space(), DeleteList::new(targets)])
    }
}

// TODO(cnpryer): Impl FormatRuleWithOptions Parenthesize
#[derive(Debug)]
struct DeleteList<'a> {
    delete_list: &'a [Expr],
}

impl<'a> DeleteList<'a> {
    const fn new(delete_list: &'a [Expr]) -> Self {
        Self { delete_list }
    }
}

impl Format<PyFormatContext<'_>> for DeleteList<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        write!(
            f,
            [&format_with(|f| {
                let separator = text(", "); // TODO(cnpryer)
                let mut join = f.join_with(&separator);

                for element in self.delete_list {
                    join.entry(&format_with(|f| {
                        write!(f, [element.format().with_options(Parenthesize::IfBreaks)])
                    }));
                }
                join.finish()
            })]
        )
    }
}
