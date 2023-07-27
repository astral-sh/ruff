use crate::comments::{leading_comments, trailing_comments};
use crate::expression::expr_tuple::TupleParentheses;
use crate::prelude::*;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::{Comprehension, Expr, Ranged};

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, item: &Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        struct Spacer<'a>(&'a Expr);

        impl Format<PyFormatContext<'_>> for Spacer<'_> {
            fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
                if f.context().comments().has_leading_comments(self.0) {
                    soft_line_break_or_space().fmt(f)
                } else {
                    space().fmt(f)
                }
            }
        }

        let Comprehension {
            range: _,
            target,
            iter,
            ifs,
            is_async,
        } = item;

        if *is_async {
            write!(f, [text("async"), space()])?;
        }

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling_comments(item);
        let (before_target_comments, before_in_comments) = dangling_item_comments.split_at(
            dangling_item_comments
                .partition_point(|comment| comment.slice().end() < target.range().start()),
        );

        let trailing_in_comments = comments.dangling_comments(iter);

        let in_spacer = format_with(|f| {
            if before_in_comments.is_empty() {
                space().fmt(f)
            } else {
                soft_line_break_or_space().fmt(f)
            }
        });

        write!(
            f,
            [
                text("for"),
                trailing_comments(before_target_comments),
                group(&format_args!(
                    Spacer(target),
                    ExprTupleWithoutParentheses(target),
                    in_spacer,
                    leading_comments(before_in_comments),
                    text("in"),
                    trailing_comments(trailing_in_comments),
                    Spacer(iter),
                    iter.format(),
                )),
            ]
        )?;
        if !ifs.is_empty() {
            let joined = format_with(|f| {
                let mut joiner = f.join_with(soft_line_break_or_space());
                for if_case in ifs {
                    let dangling_if_comments = comments.dangling_comments(if_case);

                    let (own_line_if_comments, end_of_line_if_comments) = dangling_if_comments
                        .split_at(
                            dangling_if_comments
                                .partition_point(|comment| comment.line_position().is_own_line()),
                        );
                    joiner.entry(&group(&format_args!(
                        leading_comments(own_line_if_comments),
                        text("if"),
                        trailing_comments(end_of_line_if_comments),
                        Spacer(if_case),
                        if_case.format(),
                    )));
                }
                joiner.finish()
            });

            write!(f, [soft_line_break_or_space(), group(&joined)])?;
        }
        Ok(())
    }

    fn fmt_dangling_comments(
        &self,
        _node: &Comprehension,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // dangling comments are formatted as part of fmt_fields
        Ok(())
    }
}

struct ExprTupleWithoutParentheses<'a>(&'a Expr);

impl Format<PyFormatContext<'_>> for ExprTupleWithoutParentheses<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        match self.0 {
            Expr::Tuple(expr_tuple) => expr_tuple
                .format()
                .with_options(TupleParentheses::Never)
                .fmt(f),
            other => other.format().fmt(f),
        }
    }
}
