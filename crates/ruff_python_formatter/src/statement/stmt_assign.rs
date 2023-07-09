use crate::context::PyFormatContext;
use crate::expression::parentheses::Parenthesize;
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::formatter::Formatter;
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::Expr;
use rustpython_parser::ast::StmtAssign;

// Note: This currently does wrap but not the black way so the types below likely need to be
// replaced entirely
//

#[derive(Default)]
pub struct FormatStmtAssign;

impl FormatNodeRule<StmtAssign> for FormatStmtAssign {
    fn fmt_fields(&self, item: &StmtAssign, f: &mut PyFormatter) -> FormatResult<()> {
        let StmtAssign {
            range: _,
            targets,
            value,
            type_comment: _,
        } = item;
        write!(
            f,
            [
                LhsAssignList::new(targets),
                value.format().with_options(Parenthesize::IfBreaks)
            ]
        )
    }
}

#[derive(Debug)]
struct LhsAssignList<'a> {
    lhs_assign_list: &'a [Expr],
}

impl<'a> LhsAssignList<'a> {
    const fn new(lhs_assign_list: &'a [Expr]) -> Self {
        Self { lhs_assign_list }
    }
}

impl Format<PyFormatContext<'_>> for LhsAssignList<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        for element in self.lhs_assign_list {
            write!(f, [&element.format(), space(), text("="), space(),])?;
        }
        Ok(())
    }
}
