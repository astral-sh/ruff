use crate::builders::PyFormatterExtensions;
use crate::comments::{dangling_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{format_with, group, soft_block_indent, text};
use ruff_formatter::{write, Buffer, FormatResult};
use rustpython_parser::ast::ExprCall;

#[derive(Default)]
pub struct FormatExprCall;

impl FormatNodeRule<ExprCall> for FormatExprCall {
    fn fmt_fields(&self, item: &ExprCall, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprCall {
            range: _,
            func,
            args,
            keywords,
        } = item;

        // We have a case with `f()` without any argument, which is a special case because we can
        // have a comment with no node attachment inside:
        // ```python
        // f(
        //      # This function has a dangling comment
        // )
        // ```
        if args.is_empty() && keywords.is_empty() {
            let comments = f.context().comments().clone();
            let comments = comments.dangling_comments(item);
            return write!(
                f,
                [
                    func.format(),
                    text("("),
                    dangling_comments(comments),
                    text(")")
                ]
            );
        }

        let all_args = format_with(|f| {
            f.join_comma_separated()
                .entries(
                    // We have the parentheses from the call so the arguments never need any
                    args.iter()
                        .map(|arg| (arg, arg.format().with_options(Parenthesize::Never))),
                )
                .nodes(keywords.iter())
                .finish()
        });

        write!(
            f,
            [
                func.format(),
                text("("),
                // The outer group is for things like
                // ```python
                // get_collection(
                //     hey_this_is_a_very_long_call,
                //     it_has_funny_attributes_asdf_asdf,
                //     too_long_for_the_line,
                //     really=True,
                // )
                // ```
                // The inner group is for things like:
                // ```python
                // get_collection(
                //     hey_this_is_a_very_long_call, it_has_funny_attributes_asdf_asdf, really=True
                // )
                // ```
                // TODO(konstin): Doesn't work see wrongly formatted test
                &group(&soft_block_indent(&group(&all_args))),
                text(")")
            ]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprCall, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprCall {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
