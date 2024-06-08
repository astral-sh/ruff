use ruff_formatter::{format_args, write, Buffer, FormatResult};
use ruff_python_ast::{Comprehension, Expr};
use ruff_python_trivia::{find_only_token_in_range, SimpleTokenKind};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{leading_comments, trailing_comments};
use crate::expression::expr_tuple::TupleParentheses;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, item: &Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        struct Spacer<'a>(&'a Expr);

        impl Format<PyFormatContext<'_>> for Spacer<'_> {
            fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
                if f.context().comments().has_leading(self.0) {
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
            write!(f, [token("async"), space()])?;
        }

        let comments = f.context().comments().clone();
        let dangling_item_comments = comments.dangling(item);
        let (before_target_comments, dangling_comments) = dangling_item_comments.split_at(
            dangling_item_comments.partition_point(|comment| comment.end() < target.start()),
        );

        let in_token = find_only_token_in_range(
            TextRange::new(target.end(), iter.start()),
            SimpleTokenKind::In,
            f.context().source(),
        );

        let (before_in_comments, dangling_comments) = dangling_comments.split_at(
            dangling_comments.partition_point(|comment| comment.end() < in_token.start()),
        );

        let (trailing_in_comments, dangling_if_comments) = dangling_comments
            .split_at(dangling_comments.partition_point(|comment| comment.start() < iter.start()));

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
                token("for"),
                trailing_comments(before_target_comments),
                Spacer(target),
                ExprTupleWithoutParentheses(target),
                in_spacer,
                leading_comments(before_in_comments),
                token("in"),
                trailing_comments(trailing_in_comments),
                Spacer(iter),
                iter.format(),
            ]
        )?;

        if !ifs.is_empty() {
            let joined = format_with(|f| {
                let mut joiner = f.join_with(soft_line_break_or_space());
                let mut dangling_if_comments = dangling_if_comments;

                for if_case in ifs {
                    let (if_comments, rest) = dangling_if_comments.split_at(
                        dangling_if_comments
                            .partition_point(|comment| comment.start() < if_case.start()),
                    );

                    let (own_line_if_comments, end_of_line_if_comments) = if_comments.split_at(
                        if_comments
                            .partition_point(|comment| comment.line_position().is_own_line()),
                    );

                    joiner.entry(&format_args!(
                        leading_comments(own_line_if_comments),
                        token("if"),
                        trailing_comments(end_of_line_if_comments),
                        Spacer(if_case),
                        if_case.format(),
                    ));

                    dangling_if_comments = rest;
                }

                debug_assert!(dangling_if_comments.is_empty());

                joiner.finish()
            });

            write!(f, [soft_line_break_or_space(), joined])?;
        }
        Ok(())
    }
}

struct ExprTupleWithoutParentheses<'a>(&'a Expr);

impl Format<PyFormatContext<'_>> for ExprTupleWithoutParentheses<'_> {
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        match self.0 {
            Expr::Tuple(expr_tuple) => expr_tuple
                .format()
                .with_options(TupleParentheses::Never)
                .fmt(f),
            other => other.format().fmt(f),
        }
    }
}
