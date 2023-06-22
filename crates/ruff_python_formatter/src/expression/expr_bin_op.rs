use crate::comments::{trailing_comments, Comments};
use crate::expression::binary_like::{BinaryLayout, FormatBinaryLike};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parenthesize,
};
use crate::expression::Parentheses;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use rustpython_parser::ast::{
    Constant, Expr, ExprAttribute, ExprBinOp, ExprConstant, ExprUnaryOp, Operator, UnaryOp,
};

#[derive(Default)]
pub struct FormatExprBinOp {
    parentheses: Option<Parentheses>,
}

impl FormatRuleWithOptions<ExprBinOp, PyFormatContext<'_>> for FormatExprBinOp {
    type Options = Option<Parentheses>;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatNodeRule<ExprBinOp> for FormatExprBinOp {
    fn fmt_fields(&self, item: &ExprBinOp, f: &mut PyFormatter) -> FormatResult<()> {
        item.fmt_binary(self.parentheses, f)
    }

    fn fmt_dangling_comments(&self, _node: &ExprBinOp, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled inside of `fmt_fields`
        Ok(())
    }
}

impl<'ast> FormatBinaryLike<'ast> for ExprBinOp {
    type FormatOperator = FormatOwnedWithRule<Operator, FormatOperator, PyFormatContext<'ast>>;

    fn fmt_default(&self, f: &mut PyFormatter<'ast, '_>) -> FormatResult<()> {
        let ExprBinOp {
            range: _,
            left,
            op,
            right,
        } = self;

        let comments = f.context().comments().clone();
        let operator_comments = comments.dangling_comments(self);
        let needs_space = !is_simple_power_expression(self);

        let before_operator_space = if needs_space {
            soft_line_break_or_space()
        } else {
            soft_line_break()
        };

        write!(
            f,
            [
                left.format(),
                before_operator_space,
                op.format(),
                trailing_comments(operator_comments),
            ]
        )?;

        // Format the operator on its own line if the right side has any leading comments.
        if comments.has_leading_comments(right.as_ref()) {
            write!(f, [hard_line_break()])?;
        } else if needs_space {
            write!(f, [space()])?;
        }

        write!(f, [group(&right.format())])
    }

    fn left(&self) -> FormatResult<&Expr> {
        Ok(&self.left)
    }

    fn right(&self) -> FormatResult<&Expr> {
        Ok(&self.right)
    }

    fn operator(&self) -> Self::FormatOperator {
        self.op.into_format()
    }
}

const fn is_simple_power_expression(expr: &ExprBinOp) -> bool {
    expr.op.is_pow() && is_simple_power_operand(&expr.left) && is_simple_power_operand(&expr.right)
}

/// Return `true` if an [`Expr`] adheres to [Black's definition](https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#line-breaks-binary-operators)
/// of a non-complex expression, in the context of a power operation.
const fn is_simple_power_operand(expr: &Expr) -> bool {
    match expr {
        Expr::UnaryOp(ExprUnaryOp {
            op: UnaryOp::Not, ..
        }) => false,
        Expr::Constant(ExprConstant {
            value: Constant::Complex { .. } | Constant::Float(_) | Constant::Int(_),
            ..
        }) => true,
        Expr::Name(_) => true,
        Expr::UnaryOp(ExprUnaryOp { operand, .. }) => is_simple_power_operand(operand),
        Expr::Attribute(ExprAttribute { value, .. }) => is_simple_power_operand(value),
        _ => false,
    }
}

#[derive(Copy, Clone)]
pub struct FormatOperator;

impl<'ast> AsFormat<PyFormatContext<'ast>> for Operator {
    type Format<'a> = FormatRefWithRule<'a, Operator, FormatOperator, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatOperator)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Operator {
    type Format = FormatOwnedWithRule<Operator, FormatOperator, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatOperator)
    }
}

impl FormatRule<Operator, PyFormatContext<'_>> for FormatOperator {
    fn fmt(&self, item: &Operator, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let operator = match item {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mult => "*",
            Operator::MatMult => "@",
            Operator::Div => "/",
            Operator::Mod => "%",
            Operator::Pow => "**",
            Operator::LShift => "<<",
            Operator::RShift => ">>",
            Operator::BitOr => "|",
            Operator::BitXor => "^",
            Operator::BitAnd => "&",
            Operator::FloorDiv => "//",
        };

        text(operator).fmt(f)
    }
}

impl NeedsParentheses for ExprBinOp {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => {
                if self.binary_layout() == BinaryLayout::Default
                    || comments.has_leading_comments(self.right.as_ref())
                    || comments.has_dangling_comments(self)
                {
                    Parentheses::Optional
                } else {
                    Parentheses::Custom
                }
            }
            parentheses => parentheses,
        }
    }
}
