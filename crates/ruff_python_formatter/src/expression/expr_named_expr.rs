use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprNamedExpr;

#[derive(Default)]
pub struct FormatExprNamedExpr;

impl FormatNodeRule<ExprNamedExpr> for FormatExprNamedExpr {
    fn fmt_fields(&self, item: &ExprNamedExpr, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprNamedExpr {
            target,
            value,
            range: _,
        } = item;
        write!(
            f,
            [
                target.format(),
                space(),
                text(":="),
                space(),
                value.format(),
            ]
        )
    }
}

impl NeedsParentheses for ExprNamedExpr {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        // Unlike tuples, named expression parentheses are not part of the range even when
        // mandatory. See [PEP 572](https://peps.python.org/pep-0572/) for details.
        OptionalParentheses::Always
    }
}
