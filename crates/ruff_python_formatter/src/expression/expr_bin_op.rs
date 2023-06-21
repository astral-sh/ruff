use crate::comments::{trailing_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parenthesize,
};
use crate::expression::Parentheses;
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{
    format_args, write, FormatOwnedWithRule, FormatRefWithRule, FormatRuleWithOptions,
};
use ruff_python_ast::node::AstNode;
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
        let ExprBinOp {
            left,
            right,
            op,
            range: _,
        } = item;

        let layout = if self.parentheses == Some(Parentheses::Custom) {
            BinaryLayout::from(item)
        } else {
            BinaryLayout::Default
        };

        match layout {
            BinaryLayout::Default => {
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

                // Format the operator on its own line if the right side has any leading comments.
                if comments.has_leading_comments(right.as_ref()) {
                    write!(f, [hard_line_break()])?;
                } else if needs_space {
                    write!(f, [space()])?;
                }

                write!(f, [group(&right.format())])
            }

            BinaryLayout::ExpandLeft => {
                let left = left.format().memoized();
                let right = right.format().memoized();
                write!(
                    f,
                    [best_fitting![
                        // Everything on a single line
                        format_args![left, space(), op.format(), space(), right],
                        // Break the left over multiple lines, keep the right flat
                        format_args![
                            group(&left).should_expand(true),
                            space(),
                            op.format(),
                            space(),
                            right
                        ],
                        // The content doesn't fit, indent the content and break before the operator.
                        format_args![
                            text("("),
                            block_indent(&format_args![
                                left,
                                hard_line_break(),
                                op.format(),
                                space(),
                                right
                            ]),
                            text(")")
                        ]
                    ]
                    .with_mode(BestFittingMode::AllLines)]
                )
            }

            BinaryLayout::ExpandRight => {
                let left_group = f.group_id("BinaryLeft");

                write!(
                    f,
                    [
                        // Wrap the left in a group and gives it an id. The printer first breaks the
                        // right side if `right` contains any line break because the printer breaks
                        // sequences of groups from right to left.
                        // Indents the left side if the group breaks.
                        group(&format_args![
                            if_group_breaks(&text("(")),
                            indent_if_group_breaks(
                                &format_args![
                                    soft_line_break(),
                                    left.format(),
                                    soft_line_break_or_space(),
                                    op.format(),
                                    space()
                                ],
                                left_group
                            )
                        ])
                        .with_group_id(Some(left_group)),
                        // Wrap the right in a group and indents its content but only if the left side breaks
                        group(&indent_if_group_breaks(&right.format(), left_group)),
                        // If the left side breaks, insert a hard line break to finish the indent and close the open paren.
                        if_group_breaks(&format_args![hard_line_break(), text(")")])
                            .with_group_id(Some(left_group))
                    ]
                )
            }

            BinaryLayout::ExpandRightThenLeft => {
                // The formatter expands group-sequences from right to left, and expands both if
                // there isn't enough space when expanding only one of them.
                write!(
                    f,
                    [
                        group(&left.format()),
                        space(),
                        op.format(),
                        space(),
                        group(&right.format())
                    ]
                )
            }
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
                if BinaryLayout::from(self) == BinaryLayout::Default
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum BinaryLayout {
    /// Put each operand on their own line if either side expands
    Default,

    /// Try to expand the left to make it fit. Add parentheses if the left or right don't fit.
    ///
    ///```python
    /// [
    ///     a,
    ///     b
    /// ] & c
    ///```
    ExpandLeft,

    /// Try to expand the right to make it fix. Add parentheses if the left or right don't fit.
    ///
    /// ```python
    /// a & [
    ///     b,
    ///     c
    /// ]
    /// ```
    ExpandRight,

    /// Both the left and right side can be expanded. Try in the following order:
    /// * expand the right side
    /// * expand the left side
    /// * expand both sides
    ///
    /// to make the expression fit
    ///
    /// ```python
    /// [
    ///     a,
    ///     b
    /// ] & [
    ///     c,
    ///     d
    /// ]
    /// ```
    ExpandRightThenLeft,
}

impl BinaryLayout {
    fn from(expr: &ExprBinOp) -> Self {
        match (can_break(&expr.left), can_break(&expr.right)) {
            (false, false) => Self::Default,
            (true, false) => Self::ExpandLeft,
            (false, true) => Self::ExpandRight,
            (true, true) => Self::ExpandRightThenLeft,
        }
    }
}

fn can_break(expr: &Expr) -> bool {
    use ruff_python_ast::prelude::*;

    match expr {
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
        Expr::Call(ExprCall { args, keywords, .. }) => !(args.is_empty() && keywords.is_empty()),
        Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::GeneratorExp(_) => true,
        _ => false,
    }
}
