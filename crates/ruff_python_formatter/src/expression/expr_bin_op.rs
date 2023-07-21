use crate::comments::{trailing_comments, trailing_node_comments};
use crate::expression::parentheses::{
    in_parentheses_only_group, in_parentheses_only_soft_line_break,
    in_parentheses_only_soft_line_break_or_space, is_expression_parenthesized, NeedsParentheses,
    OptionalParentheses,
};
use crate::expression::Parentheses;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions};
use ruff_python_ast::node::{AnyNodeRef, AstNode};
use rustpython_parser::ast::{
    Constant, Expr, ExprAttribute, ExprBinOp, ExprConstant, ExprUnaryOp, Operator, UnaryOp,
};
use smallvec::SmallVec;
use std::iter;

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
        let comments = f.context().comments().clone();

        let format_inner = format_with(|f: &mut PyFormatter| {
            let source = f.context().source();
            let binary_chain: SmallVec<[&ExprBinOp; 4]> = iter::successors(Some(item), |parent| {
                parent.left.as_bin_op_expr().and_then(|bin_expression| {
                    if is_expression_parenthesized(bin_expression.as_any_node_ref(), source) {
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

                let operator_comments = comments.dangling_comments(current);
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
                if comments.has_leading_comments(right.as_ref()) || !operator_comments.is_empty() {
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

    fn fmt_dangling_comments(&self, _node: &ExprBinOp, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled inside of `fmt_fields`
        Ok(())
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
        parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        if parent.is_expr_await() && !self.op.is_pow() {
            OptionalParentheses::Always
        } else {
            OptionalParentheses::Multiline
        }
    }
}
