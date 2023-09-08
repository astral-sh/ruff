use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprLambda;

use crate::comments::{dangling_comments, SourceComment};
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::other::parameters::ParametersParentheses;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprLambda;

impl FormatNodeRule<ExprLambda> for FormatExprLambda {
    fn fmt_fields(&self, item: &ExprLambda, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprLambda {
            range: _,
            parameters,
            body,
        } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [token("lambda")])?;

        if let Some(parameters) = parameters {
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

        write!(f, [token(":")])?;

        if dangling.is_empty() {
            write!(f, [space()])?;
        } else {
            write!(f, [dangling_comments(dangling)])?;
        }

        // Insert hard line break if body has leading comment to ensure consistent formatting
        if comments.has_leading(body.as_ref()) {
            write!(f, [hard_line_break()])?;
        }

        write!(f, [body.format()])
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
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
