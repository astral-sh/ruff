use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::ExprLambda;

use crate::comments::{dangling_comments, SourceComment};
use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::other::parameters::ParametersParentheses;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};

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

        write!(f, [text("lambda")])?;

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

        write!(f, [text(":")])?;

        if dangling.is_empty() {
            write!(f, [space()])?;
        } else {
            write!(f, [dangling_comments(dangling)])?;
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
