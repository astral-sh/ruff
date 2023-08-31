use std::iter;

use smallvec::SmallVec;

use ruff_formatter::{format_args, write, FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{
    Constant, Expr, ExprAttribute, ExprBinOp, ExprConstant, ExprUnaryOp, Operator, StringConstant,
    UnaryOp,
};

use crate::comments::{trailing_comments, trailing_node_comments, SourceComment};
use crate::expression::expr_constant::{is_multiline_string, ExprConstantLayout};
use crate::expression::has_parentheses;
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break,
    in_parentheses_only_soft_line_break_or_space, is_expression_parenthesized, parenthesized,
    NeedsParentheses, OptionalParentheses,
};
use crate::expression::string::StringLayout;
use crate::expression::OperatorPrecedence;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprBinOp;

impl FormatNodeRule<ExprBinOp> for FormatExprBinOp {
    fn fmt_fields(&self, item: &ExprBinOp, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();

        match Self::layout(item, f.context()) {
            BinOpLayout::LeftString(expression) => {
                let right_has_leading_comment = comments.has_leading(item.right.as_ref());

                let format_right_and_op = format_with(|f| {
                    if right_has_leading_comment {
                        space().fmt(f)?;
                    } else {
                        soft_line_break_or_space().fmt(f)?;
                    }

                    item.op.format().fmt(f)?;

                    if right_has_leading_comment {
                        hard_line_break().fmt(f)?;
                    } else {
                        space().fmt(f)?;
                    }

                    group(&item.right.format()).fmt(f)
                });

                let format_left = format_with(|f: &mut PyFormatter| {
                    let format_string =
                        expression.format().with_options(ExprConstantLayout::String(
                            StringLayout::ImplicitConcatenatedBinaryLeftSide,
                        ));

                    if is_expression_parenthesized(expression.into(), f.context().source()) {
                        parenthesized("(", &format_string, ")").fmt(f)
                    } else {
                        format_string.fmt(f)
                    }
                });

                group(&format_args![format_left, group(&format_right_and_op)]).fmt(f)
            }
            BinOpLayout::Default => {
                let format_inner = format_with(|f: &mut PyFormatter| {
                    let source = f.context().source();
                    let precedence = OperatorPrecedence::from(item.op);
                    let binary_chain: SmallVec<[&ExprBinOp; 4]> =
                        iter::successors(Some(item), |parent| {
                            parent.left.as_bin_op_expr().and_then(|bin_expression| {
                                if OperatorPrecedence::from(bin_expression.op) != precedence
                                    || is_expression_parenthesized(bin_expression.into(), source)
                                {
                                    None
                                } else {
                                    Some(bin_expression)
                                }
                            })
                        })
                        .collect();

                    // SAFETY: `binary_chain` is guaranteed not to be empty because it always contains the current expression.
                    let left_most = binary_chain.last().unwrap();

                    // Format the left most expression
                    in_parentheses_only_group(&left_most.left.format()).fmt(f)?;

                    // Iterate upwards in the binary expression tree and, for each level, format the operator
                    // and the right expression.
                    for current in binary_chain.into_iter().rev() {
                        let ExprBinOp {
                            range: _,
                            left: _,
                            op,
                            right,
                        } = current;

                        let operator_comments = comments.dangling(current);
                        let needs_space = !is_simple_power_expression(current);

                        let before_operator_space = if needs_space {
                            in_parentheses_only_soft_line_break_or_space()
                        } else {
                            in_parentheses_only_soft_line_break()
                        };

                        write!(
                            f,
                            [
                                before_operator_space,
                                op.format(),
                                trailing_comments(operator_comments),
                            ]
                        )?;

                        // Format the operator on its own line if the right side has any leading comments.
                        if comments.has_leading(right.as_ref()) || !operator_comments.is_empty() {
                            hard_line_break().fmt(f)?;
                        } else if needs_space {
                            space().fmt(f)?;
                        }

                        in_parentheses_only_group(&right.format()).fmt(f)?;

                        // It's necessary to format the trailing comments because the code bypasses
                        // `FormatNodeRule::fmt` for the nested binary expressions.
                        // Don't call the formatting function for the most outer binary expression because
                        // these comments have already been formatted.
                        if current != item {
                            trailing_node_comments(current).fmt(f)?;
                        }
                    }

                    Ok(())
                });

                in_parentheses_only_group(&format_inner).fmt(f)
            }
        }
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled inside of `fmt_fields`
        Ok(())
    }
}

impl FormatExprBinOp {
    fn layout<'a>(bin_op: &'a ExprBinOp, context: &PyFormatContext) -> BinOpLayout<'a> {
        if let Some(
            constant @ ExprConstant {
                value:
                    Constant::Str(StringConstant {
                        implicit_concatenated: true,
                        ..
                    }),
                ..
            },
        ) = bin_op.left.as_constant_expr()
        {
            let comments = context.comments();

            if bin_op.op == Operator::Mod
                && context.node_level().is_parenthesized()
                && !comments.has_dangling(constant)
                && !comments.has_dangling(bin_op)
            {
                BinOpLayout::LeftString(constant)
            } else {
                BinOpLayout::Default
            }
        } else {
            BinOpLayout::Default
        }
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

#[derive(Copy, Clone, Debug)]
enum BinOpLayout<'a> {
    Default,

    /// Specific layout for an implicit concatenated string using the "old" c-style formatting.
    ///
    /// ```python
    /// (
    ///     "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa %s"
    ///     "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb %s" % (a, b)
    /// )
    /// ```
    ///
    /// Prefers breaking the string parts over breaking in front of the `%` because it looks better if it
    /// is kept on the same line.
    LeftString(&'a ExprConstant),
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
    fn fmt(&self, item: &Operator, f: &mut PyFormatter) -> FormatResult<()> {
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
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() && !self.op.is_pow() {
            OptionalParentheses::Always
        } else if let Expr::Constant(constant) = self.left.as_ref() {
            // Multiline strings are guaranteed to never fit, avoid adding unnecessary parentheses
            if !constant.value.is_implicit_concatenated()
                && is_multiline_string(constant, context.source())
                && has_parentheses(&self.right, context).is_some()
                && !context.comments().has_dangling(self)
                && !context.comments().has(self.left.as_ref())
                && !context.comments().has(self.right.as_ref())
            {
                OptionalParentheses::Never
            } else {
                OptionalParentheses::Multiline
            }
        } else {
            OptionalParentheses::Multiline
        }
    }
}
