use crate::builders::PyFormatterExtensions;
use crate::comments::dangling_comments;
use crate::context::PyFormatContext;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, parenthesized, NeedsParentheses, Parentheses,
    Parenthesize,
};
use crate::trivia::{SimpleTokenizer, TokenKind};
use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{format_with, group, text};
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::{Expr, ExprCall, Ranged};

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

        let all_args = format_with(|f: &mut PyFormatter| {
            let source = f.context().source();
            let mut joiner = f.join_comma_separated(item.end());
            match args.as_slice() {
                [argument] if keywords.is_empty() => {
                    let parentheses =
                        if is_single_argument_parenthesized(argument, item.end(), source) {
                            Parenthesize::Always
                        } else {
                            Parenthesize::Never
                        };
                    joiner.entry(argument, &argument.format().with_options(parentheses));
                }
                arguments => {
                    joiner.nodes(arguments).nodes(keywords.iter());
                }
            }

            joiner.finish()
        });

        write!(
            f,
            [
                func.format(),
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
                parenthesized("(", &group(&all_args), ")")
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
        context: &PyFormatContext,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, context) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}

fn is_single_argument_parenthesized(argument: &Expr, call_end: TextSize, source: &str) -> bool {
    let mut has_seen_r_paren = false;

    for token in
        SimpleTokenizer::new(source, TextRange::new(argument.end(), call_end)).skip_trivia()
    {
        match token.kind() {
            TokenKind::RParen => {
                if has_seen_r_paren {
                    return true;
                }
                has_seen_r_paren = true;
            }
            // Skip over any trailing comma
            TokenKind::Comma => continue,
            _ => {
                // Passed the arguments
                break;
            }
        }
    }

    false
}
