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

        let comments = f.context().comments().clone();
        let dangling_comments = comments.dangling_comments(item);
        let leading_attribute_comments_start =
            dangling_comments.partition_point(|comment| comment.line_position().is_end_of_line());
        let (trailing_dot_comments, leading_attribute_comments) =
            dangling_comments.split_at(leading_attribute_comments_start);

        if needs_parentheses {
            value.format().with_options(Parenthesize::Always).fmt(f)?;
        } else if let Expr::Attribute(expr_attribute) = value.as_ref() {
            // We're in a attribute chain (`a.b.c`). The outermost node adds parentheses if
            // required, the inner ones don't need them so we skip the `Expr` formatting that
            // normally adds the parentheses.
            expr_attribute.format().fmt(f)?;
        } else {
            value.format().fmt(f)?;
        }

        if comments.has_trailing_own_line_comments(value.as_ref()) {
            hard_line_break().fmt(f)?;
        }

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

/// Checks if there are any own line comments in an attribute chain (a.b.c). This method is
/// recursive up to the innermost expression that the attribute chain starts behind.
fn has_breaking_comments_attribute_chain(
    expr_attribute: &ExprAttribute,
    comments: &Comments,
) -> bool {
    if comments
        .dangling_comments(expr_attribute)
        .iter()
        .any(|comment| comment.line_position().is_own_line())
        || comments.has_trailing_own_line_comments(expr_attribute)
    {
        return true;
    }

    if let Expr::Attribute(inner) = expr_attribute.value.as_ref() {
        return has_breaking_comments_attribute_chain(inner, comments);
    }

    return comments.has_trailing_own_line_comments(expr_attribute.value.as_ref());
}

impl NeedsParentheses for ExprAttribute {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        if has_breaking_comments_attribute_chain(self, comments) {
            return Parentheses::Always;
        }

        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
