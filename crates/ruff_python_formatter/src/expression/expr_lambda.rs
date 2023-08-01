use crate::comments::dangling_node_comments;
use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::other::parameters::ParametersParentheses;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprLambda;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, item: &ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprLambda {
            range: _,
            parameters,
            body,
        } = item;

        write!(f, [text("lambda")])?;

        if !parameters.args.is_empty() || parameters.vararg.is_some() || parameters.kwarg.is_some()
        {
            write!(
                f,
                [
                    space(),
                    parameters
                        .format()
                        .with_options(ParametersParentheses::Never),
                ]
            )?;
        }

        write!(
            f,
            [
                text(":"),
                space(),
                body.format(),
                // It's possible for some `Arguments` of `lambda`s to be assigned dangling comments.
                //
                // a = (
                //     lambda  # Dangling
                //     : 1
                // )
                dangling_node_comments(parameters.as_ref())
            ]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprLambda, _f: &mut PyFormatter) -> FormatResult<()> {
        // Override. Dangling comments are handled in `fmt_fields`.
        Ok(())
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
