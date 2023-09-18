use ruff_formatter::write;
use ruff_python_ast::{ArgOrKeyword, Arguments, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::comments::SourceComment;
use crate::expression::expr_generator_exp::GeneratorExpParentheses;
use crate::expression::parentheses::{empty_parenthesized, parenthesized, Parentheses};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatArguments;

impl FormatNodeRule<Arguments> for FormatArguments {
    fn fmt_fields(&self, item: &Arguments, f: &mut PyFormatter) -> FormatResult<()> {
        let Arguments {
            range,
            args,
            keywords,
        } = item;
        // We have a case with `f()` without any argument, which is a special case because we can
        // have a comment with no node attachment inside:
        // ```python
        // f(
        //      # This call has a dangling comment.
        // )
        // ```
        if args.is_empty() && keywords.is_empty() {
            let comments = f.context().comments().clone();
            let dangling = comments.dangling(item);
            return write!(f, [empty_parenthesized("(", dangling, ")")]);
        }

        let all_arguments = format_with(|f: &mut PyFormatter| {
            let source = f.context().source();
            let mut joiner = f.join_comma_separated(range.end());
            match args.as_slice() {
                [arg] if keywords.is_empty() => {
                    match arg {
                        Expr::GeneratorExp(generator_exp) => joiner.entry(
                            generator_exp,
                            &generator_exp
                                .format()
                                .with_options(GeneratorExpParentheses::Preserve),
                        ),
                        other => {
                            let parentheses =
                                if is_single_argument_parenthesized(arg, range.end(), source) {
                                    Parentheses::Always
                                } else {
                                    // Note: no need to handle opening-parenthesis comments, since
                                    // an opening-parenthesis comment implies that the argument is
                                    // parenthesized.
                                    Parentheses::Never
                                };
                            joiner.entry(other, &other.format().with_options(parentheses))
                        }
                    };
                }
                _ => {
                    for arg_or_keyword in item.arguments_source_order() {
                        match arg_or_keyword {
                            ArgOrKeyword::Arg(arg) => {
                                joiner.entry(arg, &arg.format());
                            }
                            ArgOrKeyword::Keyword(keyword) => {
                                joiner.entry(keyword, &keyword.format());
                            }
                        }
                    }
                }
            }

            joiner.finish()
        });

        // If the arguments are non-empty, then a dangling comment indicates a comment on the
        // same line as the opening parenthesis, e.g.:
        // ```python
        // f(  # This call has a dangling comment.
        //     a,
        //     b,
        //     c,
        // )
        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling(item);

        write!(
            f,
            [
                // The outer group is for things like:
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
                parenthesized("(", &group(&all_arguments), ")")
                    .with_dangling_comments(dangling_comments)
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
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
