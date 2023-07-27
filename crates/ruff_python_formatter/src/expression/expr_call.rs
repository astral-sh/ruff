use ruff_python_ast::{Expr, ExprCall, Ranged};
use ruff_text_size::{TextRange, TextSize};

use crate::builders::empty_parenthesized_with_dangling_comments;
use ruff_formatter::write;
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};

use crate::expression::expr_generator_exp::GeneratorExpParentheses;
use crate::expression::parentheses::{
    parenthesized, NeedsParentheses, OptionalParentheses, Parentheses,
};
use crate::prelude::*;
use crate::FormatNodeRule;

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
            return write!(
                f,
                [
                    func.format(),
                    empty_parenthesized_with_dangling_comments(
                        text("("),
                        comments.dangling_comments(item),
                        text(")"),
                    )
                ]
            );
        }

        let all_args = format_with(|f: &mut PyFormatter| {
            let source = f.context().source();
            let mut joiner = f.join_comma_separated(item.end());
            match args.as_slice() {
                [argument] if keywords.is_empty() => {
                    match argument {
                        Expr::GeneratorExp(generator_exp) => joiner.entry(
                            generator_exp,
                            &generator_exp
                                .format()
                                .with_options(GeneratorExpParentheses::StripIfOnlyFunctionArg),
                        ),
                        other => {
                            let parentheses =
                                if is_single_argument_parenthesized(argument, item.end(), source) {
                                    Parentheses::Always
                                } else {
                                    Parentheses::Never
                                };
                            joiner.entry(other, &other.format().with_options(parentheses))
                        }
                    };
                }
                arguments => {
                    joiner
                        .entries(
                            // We have the parentheses from the call so the arguments never need any
                            arguments
                                .iter()
                                .map(|arg| (arg, arg.format().with_options(Parentheses::Preserve))),
                        )
                        .nodes(keywords.iter());
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
        _parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        self.func.needs_parentheses(self.into(), context)
    }
}

fn is_single_argument_parenthesized(argument: &Expr, call_end: TextSize, source: &str) -> bool {
    let mut has_seen_r_paren = false;

    for token in
        SimpleTokenizer::new(source, TextRange::new(argument.end(), call_end)).skip_trivia()
    {
        match token.kind() {
            SimpleTokenKind::RParen => {
                if has_seen_r_paren {
                    return true;
                }
                has_seen_r_paren = true;
            }
            // Skip over any trailing comma
            SimpleTokenKind::Comma => continue,
            _ => {
                // Passed the arguments
                break;
            }
        }
    }

    false
}
