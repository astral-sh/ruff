use crate::expression::parentheses::Parenthesize;
use crate::prelude::PyFormatContext;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{
    format_args, format_with, group, soft_line_break_or_space, space, text, Formatter,
};
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
        let separator =
            format_with(|f| group(&format_args![text(","), soft_line_break_or_space(),]).fmt(f));
        let mut join = f.join_with(separator);

        for element in self.delete_list {
            join.entry(&format_with(|f| {
                write!(f, [element.format().with_options(Parenthesize::IfBreaks)])
            }));
        }
        join.finish()
    }
}
