use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::other::arguments::ArgumentsParentheses;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use rustpython_parser::ast::ExprLambda;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, item: &ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprLambda {
            range: _,
            args,
            body,
        } = item;

        write!(f, [text("lambda")])?;
        if !args.args.is_empty() {
            write!(
                f,
                [
                    space(),
                    args.format()
                        .with_options(ArgumentsParentheses::SkipInsideLambda),
                ]
            )?;
        }
        write!(f, [text(":"), space(), body.format()])
    }
}

impl NeedsParentheses for ExprLambda {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Multiline
    }
}
