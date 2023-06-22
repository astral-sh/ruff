use crate::comments::{leading_comments, trailing_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use rustpython_parser::ast::{Constant, Expr, ExprAttribute, ExprConstant};

#[derive(Default)]
pub struct FormatExprAttribute;

impl FormatNodeRule<ExprAttribute> for FormatExprAttribute {
    fn fmt_fields(&self, item: &ExprAttribute, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprAttribute {
            value,
            range: _,
            attr,
            ctx: _,
        } = item;

        let needs_parentheses = matches!(
            value.as_ref(),
            Expr::Constant(ExprConstant {
                value: Constant::Int(_) | Constant::Float(_),
                ..
            })
        );

        if needs_parentheses {
            value.format().with_options(Parenthesize::Always).fmt(f)?;
        } else {
            value.format().fmt(f)?;
        }

        let comments = f.context().comments().clone();

        if comments.has_trailing_own_line_comments(value.as_ref()) {
            hard_line_break().fmt(f)?;
        }

        let dangling_comments = comments.dangling_comments(item);

        let leading_attribute_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_end_of_line());
        let (trailing_dot_comments, leading_attribute_comments) =
            dangling_comments.split_at(leading_attribute_comments_start);

        write!(
            f,
            [
                text("."),
                trailing_comments(trailing_dot_comments),
                (!leading_attribute_comments.is_empty()).then_some(hard_line_break()),
                leading_comments(leading_attribute_comments),
                attr.format()
            ]
        )
    }

    fn fmt_dangling_comments(
        &self,
        _node: &ExprAttribute,
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // handle in `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprAttribute {
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
