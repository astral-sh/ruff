use rustpython_parser::ast::StmtAssign;

use ruff_formatter::write;

use crate::expression::maybe_parenthesize_expression;
use crate::expression::parentheses::Parenthesize;
use crate::prelude::*;
use crate::FormatNodeRule;

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

        for target in targets {
            write!(f, [target.format(), space(), text("="), space()])?;
        }

        write!(
            f,
            [maybe_parenthesize_expression(
                value,
                item,
                Parenthesize::IfBreaks
            )]
        )
    }
}
