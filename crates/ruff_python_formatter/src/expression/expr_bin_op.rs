use crate::comments::trailing_comments;
use crate::expression::Parentheses;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{
    format_args, write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions,
};
use ruff_python_ast::node::AstNode;
use rustpython_parser::ast::{
    Constant, Expr, ExprAttribute, ExprBinOp, ExprConstant, ExprUnaryOp, Operator, Unaryop,
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
        let ExprBinOp {
            left,
            right,
            op,
            range: _,
        } = item;

        let should_break_right = self.parentheses == Some(Parentheses::Optional)
            && should_binary_break_right_side_first(item);

        if should_break_right {
            let left = left.format().memoized();
            let right = right.format().memoized();

            write!(
                f,
                [best_fitting![
                    // The whole expression on a single line
                    format_args![left, space(), op.format(), space(), right],
                    // Break the right, but keep the left flat
                    format_args![
                        left,
                        text(" "),
                        op.format(),
                        text(" "),
                        group(&right).should_expand(true),
                    ],
                    // Break after the operator, try to keep the right flat, otherwise expand it
                    format_args![
                        text("("),
                        block_indent(&format_args![
                            left,
                            hard_line_break(),
                            op.format(),
                            space(),
                            group(&right),
                        ]),
                        text(")")
                    ],
                ]]
            )
        } else {
            let comments = f.context().comments().clone();
            let operator_comments = comments.dangling_comments(item.as_any_node_ref());
            let needs_space = !is_simple_power_expression(item);

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

            // Format the operator on its own line if it has any trailing comments at the right side has leading comments.
            if !operator_comments.is_empty() && comments.has_leading_comments(right.as_ref().into())
            {
                write!(f, [hard_line_break()])?;
            } else if needs_space {
                write!(f, [space()])?;
            }

            write!(f, [group(&right.format())])
        }
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
            op: Unaryop::Not, ..
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

pub(super) fn should_binary_break_right_side_first(expr: &ExprBinOp) -> bool {
    use ruff_python_ast::prelude::*;

    if expr.left.is_bin_op_expr() {
        false
    } else {
        match expr.right.as_ref() {
            Expr::Tuple(ExprTuple {
                elts: expressions, ..
            })
            | Expr::List(ExprList {
                elts: expressions, ..
            })
            | Expr::Set(ExprSet {
                elts: expressions, ..
            })
            | Expr::Dict(ExprDict {
                values: expressions,
                ..
            }) => !expressions.is_empty(),
            Expr::Call(ExprCall { args, keywords, .. }) => !args.is_empty() && !keywords.is_empty(),
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::GeneratorExp(_) => {
                true
            }
            _ => false,
        }
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
