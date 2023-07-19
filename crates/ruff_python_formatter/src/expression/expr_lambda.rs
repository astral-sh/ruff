use crate::comments::dangling_comments;
use crate::context::PyFormatContext;
use crate::expression::parentheses::{NeedsParentheses, OptionalParentheses};
use crate::other::arguments::ArgumentsParentheses;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
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

        // It's possible for some `Arguments` of `lambda`s to be assigned dangling comments.
        //
        // a = (
        //     lambda  # Dangling
        //     : 1
        // )
        let comments = f.context().comments().clone();
        let dangling = comments.dangling_comments(args.as_any_node_ref());

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

        write!(
            f,
            [
                text(":"),
                space(),
                body.format(),
                dangling_comments(dangling)
            ]
        )
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
