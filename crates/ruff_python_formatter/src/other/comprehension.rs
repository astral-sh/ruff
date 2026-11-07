use ruff_formatter::{Buffer, FormatResult, format_args, write};
use ruff_python_ast::token::parentheses_iterator;
use ruff_python_ast::{Comprehension, Expr};
use ruff_python_trivia::{SimpleTokenKind, find_only_token_in_range};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{leading_comments, trailing_comments};
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{Parentheses, Parenthesize};
use crate::expression::{FormatUnpackingTarget, maybe_parenthesize_expression};
use crate::prelude::*;
use crate::preview::is_wrap_comprehension_in_enabled;

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, item: &Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        struct Spacer<'a> {
            expression: &'a Expr,
            preserve_parentheses: bool,
        }

        impl Format<PyFormatContext<'_>> for Spacer<'_> {
            fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
                let has_leading_comments = f.context().comments().has_leading(self.expression);

                // Don't add a soft line break for parenthesized expressions with a leading comment.
                // The comments are rendered **inside** the parentheses and adding a softline break
                // unnecessarily forces the parentheses to be on their own line.
                // ```python
                // y = [
                //    ...
                //    if
                //    (
                //          # See how the `(` gets forced on its own line? We don't want that.
                //         ...
                //     )
                // ]
                // ```
                let mut will_be_parenthesized = self.preserve_parentheses
                    && f.context()
                        .is_expression_parenthesized(self.expression.into());

                if will_be_parenthesized
                    && is_wrap_comprehension_in_enabled(f.context())
                    && let Some(first) = f.context().comments().leading(self.expression).first()
                    && let Some(parentheses) =
                        parentheses_iterator(self.expression.into(), None, f.context().tokens())
                            .last()
                    && first.start() < parentheses.start()
                {
                    // A comment before the opening parenthesis stays outside it
                    // and therefore needs a line break after the keyword.
                    will_be_parenthesized = false;
                }

                if has_leading_comments && !will_be_parenthesized {
                    soft_line_break_or_space().fmt(f)
                } else {
                    space().fmt(f)
                }
            }
        }

        let Comprehension {
            range: _,
            node_index: _,
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
                Spacer {
                    expression: target,
                    preserve_parentheses: !target.is_tuple_expr()
                },
                ExprTupleWithoutParentheses(target),
                in_spacer,
                leading_comments(before_in_comments),
                token("in"),
            ]
        )?;

        if is_wrap_comprehension_in_enabled(f.context()) {
            if trailing_in_comments
                .iter()
                .any(|comment| comment.line_position().is_own_line())
            {
                let (end_of_line, own_line) = trailing_in_comments.split_at(
                    trailing_in_comments
                        .partition_point(|comment| comment.line_position().is_end_of_line()),
                );
                write!(
                    f,
                    [
                        trailing_comments(end_of_line),
                        hard_line_break(),
                        leading_comments(own_line),
                        iter.format()
                    ]
                )?;
            } else if !trailing_in_comments.is_empty() || comments.contains_comments(iter.into()) {
                write!(
                    f,
                    [
                        trailing_comments(trailing_in_comments),
                        Spacer {
                            expression: iter,
                            preserve_parentheses: true
                        },
                        iter.format()
                    ]
                )?;
            } else {
                space().fmt(f)?;
                if matches!(
                    iter,
                    Expr::If(_)
                        | Expr::Lambda(_)
                        | Expr::Yield(_)
                        | Expr::YieldFrom(_)
                        | Expr::Named(_)
                ) {
                    // Without parentheses, trailing clauses can become part of a
                    // lambda body or be parsed as a conditional expression.
                    iter.format().with_options(Parentheses::Always).fmt(f)?;
                } else if f.context().is_expression_parenthesized(iter.into())
                    && matches!(iter, Expr::BinOp(_) | Expr::BoolOp(_) | Expr::Compare(_))
                {
                    iter.format().fmt(f)?;
                } else {
                    // The comprehension's brackets permit line breaks, but a long
                    // iterable can fit better when it has its own parentheses.
                    // Only try this once the comprehension itself has expanded;
                    // a best-fit boundary must not hide an iterable's line breaks
                    // from the enclosing comprehension.
                    let wrapped = format_with(|f| {
                        let mut f = WithNodeLevel::new(NodeLevel::Expression(None), f);
                        maybe_parenthesize_expression(iter, item, Parenthesize::IfBreaks)
                            .fmt(&mut f)
                    });
                    write!(
                        f,
                        [
                            if_group_fits_on_line(&iter.format().with_options(Parentheses::Never)),
                            if_group_breaks(&wrapped),
                        ]
                    )?;
                }
            }
        } else {
            write!(
                f,
                [
                    trailing_comments(trailing_in_comments),
                    Spacer {
                        expression: iter,
                        preserve_parentheses: true
                    },
                    iter.format()
                ]
            )?;
        }

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
                        Spacer {
                            expression: if_case,
                            preserve_parentheses: true
                        },
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
            other => FormatUnpackingTarget(other).fmt(f),
        }
    }
}
