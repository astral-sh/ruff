use std::num::NonZeroUsize;
use std::ops::Deref;

use smallvec::SmallVec;

use ruff_formatter::{format_args, write, FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{
    Constant, Expr, ExprAttribute, ExprBinOp, ExprConstant, ExprUnaryOp, Operator, StringConstant,
    UnaryOp,
};

use crate::comments::{leading_comments, trailing_comments, Comments, SourceComment};
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
                let comments = f.context().comments().clone();
                let chain =
                    BinaryChain::from_binary_expression(item, &comments, f.context().source());

                in_parentheses_only_group(&chain.deref()).fmt(f)
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

const fn is_simple_power_expression(left: &Expr, right: &Expr) -> bool {
    is_simple_power_operand(left) && is_simple_power_operand(right)
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

        token(operator).fmt(f)
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

#[derive(Debug)]
enum BinaryPart<'a> {
    Operand(Operand<'a>),
    Operator(OperatorPart<'a>),
}

#[derive(Debug)]
struct Operand<'a> {
    leading_comments: OperandComments<'a>,
    expression: &'a Expr,
    trailing_comments: &'a [SourceComment],
}

impl<'a> Operand<'a> {
    fn has_leading_comments(&self) -> bool {
        match self.leading_comments {
            OperandComments::Binary(comments) | OperandComments::Node(comments) => {
                !comments.is_empty()
            }
        }
    }
}

#[derive(Debug)]
enum OperandComments<'a> {
    Binary(&'a [SourceComment]),
    Node(&'a [SourceComment]),
}

#[derive(Debug)]
struct OperatorPart<'a> {
    operator: Operator,
    trailing_comments: &'a [SourceComment],
}

impl OperatorPart<'_> {
    fn precedence(&self) -> OperatorPrecedence {
        OperatorPrecedence::from(self.operator)
    }

    fn has_trailing_comments(&self) -> bool {
        !self.trailing_comments.is_empty()
    }
}

impl<'a> BinaryPart<'a> {
    fn leading_comments(&self) -> &'a [SourceComment] {
        match self {
            BinaryPart::Operand(Operand {
                leading_comments:
                    (OperandComments::Node(leading_comments)
                    | OperandComments::Binary(leading_comments)),
                ..
            }) => leading_comments,
            BinaryPart::Operator { .. } => &[],
        }
    }

    fn unwrap_operand(&self) -> &Operand<'a> {
        match self {
            BinaryPart::Operand(operand) => operand,
            BinaryPart::Operator(operator) => {
                panic!("Expected operand but found operator {operator:?}.")
            }
        }
    }
}

#[derive(Debug)]
struct BinaryChain<'a>(SmallVec<[BinaryPart<'a>; 8]>);

impl<'a> BinaryChain<'a> {
    fn from_binary_expression(
        binary: &'a ExprBinOp,
        comments: &'a Comments,
        source: &'a str,
    ) -> Self {
        enum Side<'a> {
            Left(&'a ExprBinOp),
            Right(&'a ExprBinOp),
        }

        impl<'a> Side<'a> {
            fn expression(&self) -> &'a Expr {
                match self {
                    Side::Left(expression) => &expression.left,
                    Side::Right(expression) => &expression.right,
                }
            }
        }

        fn rec<'a>(
            side: Side<'a>,
            comments: &'a Comments,
            source: &'a str,
            parts: &mut SmallVec<[BinaryPart<'a>; 8]>,
        ) {
            let expression = side.expression();
            if let Expr::BinOp(binary) = expression {
                if is_expression_parenthesized(expression.into(), source) {
                    parts.push(BinaryPart::Operand(Operand {
                        leading_comments: OperandComments::Node(comments.leading(expression)),
                        expression,
                        trailing_comments: &[],
                    }));
                } else {
                    rec(Side::Left(binary), comments, source, parts);
                    parts.push(BinaryPart::Operator(OperatorPart {
                        operator: binary.op,
                        trailing_comments: comments.dangling(binary),
                    }));

                    rec(Side::Right(binary), comments, source, parts);
                }
            } else {
                let leading_comments = match side {
                    Side::Left(enclosing) => OperandComments::Binary(comments.leading(enclosing)),
                    Side::Right(_) => OperandComments::Node(comments.leading(expression)),
                };

                let trailing_comments = match side {
                    Side::Left(_) => &[],
                    Side::Right(enclosing) => comments.trailing(enclosing),
                };

                parts.push(BinaryPart::Operand(Operand {
                    leading_comments,
                    expression,
                    trailing_comments,
                }));
            }
        }

        let mut parts = SmallVec::new();
        rec(Side::Left(binary), comments, source, &mut parts);
        parts.push(BinaryPart::Operator(OperatorPart {
            operator: binary.op,
            trailing_comments: comments.dangling(binary),
        }));
        rec(Side::Right(binary), comments, source, &mut parts);

        if let Some(BinaryPart::Operand(last_operand)) = parts.last_mut() {
            last_operand.trailing_comments = &[];
        }

        Self(parts)
    }
}

impl<'a> Deref for BinaryChain<'a> {
    type Target = BinaryChainSlice<'a>;

    fn deref(&self) -> &Self::Target {
        BinaryChainSlice::from_slice(&self.0)
    }
}

#[repr(transparent)]
struct BinaryChainSlice<'a>([BinaryPart<'a>]);

impl<'a> BinaryChainSlice<'a> {
    fn from_slice<'slice>(slice: &'slice [BinaryPart<'a>]) -> &'slice Self {
        #[allow(unsafe_code)]
        unsafe {
            // SAFETY: `BinaryChainSlice` has the same layout as a slice because it uses `repr(transparent)`
            std::mem::transmute(slice)
        }
    }

    fn operators(&self) -> impl Iterator<Item = (OperatorIndex, &OperatorPart<'a>)> {
        self.0.iter().enumerate().filter_map(|(index, part)| {
            if let BinaryPart::Operator(operator) = part {
                Some((OperatorIndex::new(index), operator))
            } else {
                None
            }
        })
    }

    fn between_operators(&self, last_operator: Option<OperatorIndex>, end: OperatorIndex) -> &Self {
        let start = last_operator.map_or(0usize, |operator| operator.right_operand().0);
        Self::from_slice(&self.0[start..end.0.get()])
    }
    fn up_to_operator(&self, last_operand: OperandIndex, end: OperatorIndex) -> &Self {
        Self::from_slice(&self.0[last_operand.0..end.0.get()])
    }

    fn after_operator(&self, index: OperatorIndex) -> &Self {
        Self::from_slice(&self.0[index.right_operand().0..])
    }

    /// Returns the operator with the lowest precedence if any
    fn lowest_precedence(&self) -> OperatorPrecedence {
        self.operators()
            .map(|(_, operator)| operator.precedence())
            .max()
            .unwrap_or(OperatorPrecedence::None)
    }

    fn first_operand(&self) -> &Operand<'a> {
        match self.0.first() {
            Some(BinaryPart::Operand(operand)) => operand,
            _ => panic!("Expected an operand"),
        }
    }

    fn last_operand(&self) -> &Operand<'a> {
        match self.0.last() {
            Some(BinaryPart::Operand(operand)) => operand,
            _ => panic!("Expected an operand"),
        }
    }

    const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Format<PyFormatContext<'_>> for BinaryChainSlice<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext>) -> FormatResult<()> {
        // Single operand slice
        if let [BinaryPart::Operand(Operand { expression, .. })] = &self.0 {
            return expression.format().fmt(f);
        }

        let mut last_operator: Option<OperatorIndex> = None;

        let lowest_precedence = self.lowest_precedence();

        for (index, operator_part) in self.operators() {
            if operator_part.precedence() == lowest_precedence {
                let left = self.between_operators(last_operator, index);
                let right = self.after_operator(index);

                let is_pow = operator_part.operator.is_pow()
                    && is_simple_power_expression(
                        left.last_operand().expression,
                        right.first_operand().expression,
                    );

                if let OperandComments::Binary(leading) = left.first_operand().leading_comments {
                    leading_comments(leading).fmt(f)?;
                }

                in_parentheses_only_group(&left).fmt(f)?;

                trailing_comments(left.last_operand().trailing_comments).fmt(f)?;

                if is_pow {
                    in_parentheses_only_soft_line_break().fmt(f)?;
                } else {
                    in_parentheses_only_soft_line_break_or_space().fmt(f)?;
                }

                operator_part.operator.format().fmt(f)?;
                trailing_comments(operator_part.trailing_comments).fmt(f)?;

                // Format the operator on its own line if the right side has any leading comments.
                // if comments.has_leading(right.as_ref()) || !operator_comments.is_empty() {
                if right.first_operand().has_leading_comments()
                    || operator_part.has_trailing_comments()
                {
                    hard_line_break().fmt(f)?;
                } else if !is_pow {
                    space().fmt(f)?;
                }

                last_operator = Some(index);
            }
        }

        // Format the last right side
        // SAFETY: Last operator is guaranteed to be initialized because the slice contains at least two elements.
        let right = self.after_operator(last_operator.unwrap());

        if !right.is_empty() {
            if let OperandComments::Binary(leading) = right.first_operand().leading_comments {
                leading_comments(leading).fmt(f)?;
            }

            in_parentheses_only_group(&right).fmt(f)?;

            // trailing_comments(right.last_operand().trailing_comments).fmt(f)?;
        }

        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
struct OperandIndex(usize);

impl OperandIndex {
    fn new(index: usize) -> Self {
        debug_assert_eq!(index % 2, 0, "Operand indices must be even positions");

        Self(index)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
struct OperatorIndex(NonZeroUsize);

impl OperatorIndex {
    fn new(index: usize) -> Self {
        assert_eq!(index % 2, 1, "Operator indices must be odd positions");

        // SAFETY A value with a module 0 is guaranteed to never equal 0
        #[allow(unsafe_code)]
        Self(unsafe { NonZeroUsize::new_unchecked(index) })
    }

    fn left_operand(self) -> OperandIndex {
        OperandIndex::new(self.0.get() - 1)
    }

    fn right_operand(self) -> OperandIndex {
        OperandIndex::new(self.0.get() + 1)
    }
}
