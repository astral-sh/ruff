use std::cmp::Ordering;

use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Operator};

use ruff_formatter::{
    write, FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions,
};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::visitor::preorder::{walk_expr, PreorderVisitor};

use crate::builders::parenthesize_if_expands;
use crate::context::{NodeLevel, WithNodeLevel};
use crate::expression::parentheses::{
    is_expression_parenthesized, optional_parentheses, parenthesized, NeedsParentheses,
    OptionalParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;

pub(crate) mod expr_attribute;
pub(crate) mod expr_await;
pub(crate) mod expr_bin_op;
pub(crate) mod expr_bool_op;
pub(crate) mod expr_call;
pub(crate) mod expr_compare;
pub(crate) mod expr_constant;
pub(crate) mod expr_dict;
pub(crate) mod expr_dict_comp;
pub(crate) mod expr_formatted_value;
pub(crate) mod expr_generator_exp;
pub(crate) mod expr_if_exp;
pub(crate) mod expr_joined_str;
pub(crate) mod expr_lambda;
pub(crate) mod expr_list;
pub(crate) mod expr_list_comp;
pub(crate) mod expr_name;
pub(crate) mod expr_named_expr;
pub(crate) mod expr_set;
pub(crate) mod expr_set_comp;
pub(crate) mod expr_slice;
pub(crate) mod expr_starred;
pub(crate) mod expr_subscript;
pub(crate) mod expr_tuple;
pub(crate) mod expr_unary_op;
pub(crate) mod expr_yield;
pub(crate) mod expr_yield_from;
pub(crate) mod number;
pub(crate) mod parentheses;
pub(crate) mod string;

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub struct FormatExpr {
    parentheses: Parentheses,
}

impl FormatRuleWithOptions<Expr, PyFormatContext<'_>> for FormatExpr {
    type Options = Parentheses;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parentheses = options;
        self
    }
}

impl FormatRule<Expr, PyFormatContext<'_>> for FormatExpr {
    fn fmt(&self, expression: &Expr, f: &mut PyFormatter) -> FormatResult<()> {
        let parentheses = self.parentheses;

        let format_expr = format_with(|f| match expression {
            Expr::BoolOp(expr) => expr.format().with_options(Some(parentheses)).fmt(f),
            Expr::NamedExpr(expr) => expr.format().fmt(f),
            Expr::BinOp(expr) => expr.format().with_options(Some(parentheses)).fmt(f),
            Expr::UnaryOp(expr) => expr.format().fmt(f),
            Expr::Lambda(expr) => expr.format().fmt(f),
            Expr::IfExp(expr) => expr.format().fmt(f),
            Expr::Dict(expr) => expr.format().fmt(f),
            Expr::Set(expr) => expr.format().fmt(f),
            Expr::ListComp(expr) => expr.format().fmt(f),
            Expr::SetComp(expr) => expr.format().fmt(f),
            Expr::DictComp(expr) => expr.format().fmt(f),
            Expr::GeneratorExp(expr) => expr.format().fmt(f),
            Expr::Await(expr) => expr.format().fmt(f),
            Expr::Yield(expr) => expr.format().fmt(f),
            Expr::YieldFrom(expr) => expr.format().fmt(f),
            Expr::Compare(expr) => expr.format().with_options(Some(parentheses)).fmt(f),
            Expr::Call(expr) => expr.format().fmt(f),
            Expr::FormattedValue(expr) => expr.format().fmt(f),
            Expr::JoinedStr(expr) => expr.format().fmt(f),
            Expr::Constant(expr) => expr.format().fmt(f),
            Expr::Attribute(expr) => expr.format().fmt(f),
            Expr::Subscript(expr) => expr.format().fmt(f),
            Expr::Starred(expr) => expr.format().fmt(f),
            Expr::Name(expr) => expr.format().fmt(f),
            Expr::List(expr) => expr.format().fmt(f),
            Expr::Tuple(expr) => expr.format().fmt(f),
            Expr::Slice(expr) => expr.format().fmt(f),
            Expr::LineMagic(_) => todo!(),
        });

        let parenthesize = match parentheses {
            Parentheses::Preserve => {
                is_expression_parenthesized(AnyNodeRef::from(expression), f.context().source())
            }
            Parentheses::Always => true,
            Parentheses::Never => false,
        };

        if parenthesize {
            parenthesized("(", &format_expr, ")").fmt(f)
        } else {
            let level = match f.context().node_level() {
                NodeLevel::TopLevel | NodeLevel::CompoundStatement => NodeLevel::Expression(None),
                saved_level @ (NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression) => {
                    saved_level
                }
            };

            let mut f = WithNodeLevel::new(level, f);

            write!(f, [format_expr])
        }
    }
}

/// Wraps an expression in an optional parentheses except if its [`NeedsParentheses::needs_parentheses`] implementation
/// indicates that it is okay to omit the parentheses. For example, parentheses can always be omitted for lists,
/// because they already bring their own parentheses.
pub(crate) fn maybe_parenthesize_expression<'a, T>(
    expression: &'a Expr,
    parent: T,
    parenthesize: Parenthesize,
) -> MaybeParenthesizeExpression<'a>
where
    T: Into<AnyNodeRef<'a>>,
{
    MaybeParenthesizeExpression {
        expression,
        parent: parent.into(),
        parenthesize,
    }
}

pub(crate) struct MaybeParenthesizeExpression<'a> {
    expression: &'a Expr,
    parent: AnyNodeRef<'a>,
    parenthesize: Parenthesize,
}

impl Format<PyFormatContext<'_>> for MaybeParenthesizeExpression<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let MaybeParenthesizeExpression {
            expression,
            parent,
            parenthesize,
        } = self;

        let comments = f.context().comments();
        let preserve_parentheses = parenthesize.is_optional()
            && is_expression_parenthesized(AnyNodeRef::from(*expression), f.context().source());

        let has_comments = comments.has_leading_comments(*expression)
            || comments.has_trailing_own_line_comments(*expression);

        if preserve_parentheses || has_comments {
            return expression.format().with_options(Parentheses::Always).fmt(f);
        }

        let needs_parentheses = expression.needs_parentheses(*parent, f.context());
        let needs_parentheses = match parenthesize {
            Parenthesize::IfRequired => {
                if !needs_parentheses.is_always() && f.context().node_level().is_parenthesized() {
                    OptionalParentheses::Never
                } else {
                    needs_parentheses
                }
            }
            Parenthesize::Optional | Parenthesize::IfBreaks => needs_parentheses,
        };

        match needs_parentheses {
            OptionalParentheses::Multiline if *parenthesize != Parenthesize::IfRequired => {
                if can_omit_optional_parentheses(expression, f.context()) {
                    optional_parentheses(&expression.format().with_options(Parentheses::Never))
                        .fmt(f)
                } else {
                    parenthesize_if_expands(&expression.format().with_options(Parentheses::Never))
                        .fmt(f)
                }
            }
            OptionalParentheses::Always => {
                expression.format().with_options(Parentheses::Always).fmt(f)
            }
            OptionalParentheses::Never | OptionalParentheses::Multiline => {
                expression.format().with_options(Parentheses::Never).fmt(f)
            }
        }
    }
}

impl NeedsParentheses for Expr {
    fn needs_parentheses(
        &self,
        parent: AnyNodeRef,
        context: &PyFormatContext,
    ) -> OptionalParentheses {
        match self {
            Expr::BoolOp(expr) => expr.needs_parentheses(parent, context),
            Expr::NamedExpr(expr) => expr.needs_parentheses(parent, context),
            Expr::BinOp(expr) => expr.needs_parentheses(parent, context),
            Expr::UnaryOp(expr) => expr.needs_parentheses(parent, context),
            Expr::Lambda(expr) => expr.needs_parentheses(parent, context),
            Expr::IfExp(expr) => expr.needs_parentheses(parent, context),
            Expr::Dict(expr) => expr.needs_parentheses(parent, context),
            Expr::Set(expr) => expr.needs_parentheses(parent, context),
            Expr::ListComp(expr) => expr.needs_parentheses(parent, context),
            Expr::SetComp(expr) => expr.needs_parentheses(parent, context),
            Expr::DictComp(expr) => expr.needs_parentheses(parent, context),
            Expr::GeneratorExp(expr) => expr.needs_parentheses(parent, context),
            Expr::Await(expr) => expr.needs_parentheses(parent, context),
            Expr::Yield(expr) => expr.needs_parentheses(parent, context),
            Expr::YieldFrom(expr) => expr.needs_parentheses(parent, context),
            Expr::Compare(expr) => expr.needs_parentheses(parent, context),
            Expr::Call(expr) => expr.needs_parentheses(parent, context),
            Expr::FormattedValue(expr) => expr.needs_parentheses(parent, context),
            Expr::JoinedStr(expr) => expr.needs_parentheses(parent, context),
            Expr::Constant(expr) => expr.needs_parentheses(parent, context),
            Expr::Attribute(expr) => expr.needs_parentheses(parent, context),
            Expr::Subscript(expr) => expr.needs_parentheses(parent, context),
            Expr::Starred(expr) => expr.needs_parentheses(parent, context),
            Expr::Name(expr) => expr.needs_parentheses(parent, context),
            Expr::List(expr) => expr.needs_parentheses(parent, context),
            Expr::Tuple(expr) => expr.needs_parentheses(parent, context),
            Expr::Slice(expr) => expr.needs_parentheses(parent, context),
            Expr::LineMagic(_) => todo!(),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Expr {
    type Format<'a> = FormatRefWithRule<'a, Expr, FormatExpr, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatExpr::default())
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Expr {
    type Format = FormatOwnedWithRule<Expr, FormatExpr, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatExpr::default())
    }
}

/// Tests if it is safe to omit the optional parentheses.
///
/// We prefer parentheses at least in the following cases:
/// * The expression contains more than one unparenthesized expression with the same priority. For example,
///     the expression `a * b * c` contains two multiply operations. We prefer parentheses in that case.
///     `(a * b) * c` or `a * b + c` are okay, because the subexpression is parenthesized, or the expression uses operands with a lower priority
/// * The expression contains at least one parenthesized sub expression (optimization to avoid unnecessary work)
///
/// This mimics Black's [`_maybe_split_omitting_optional_parens`](https://github.com/psf/black/blob/d1248ca9beaf0ba526d265f4108836d89cf551b7/src/black/linegen.py#L746-L820)
fn can_omit_optional_parentheses(expr: &Expr, context: &PyFormatContext) -> bool {
    let mut visitor = CanOmitOptionalParenthesesVisitor::new(context.source());
    visitor.visit_subexpression(expr);
    visitor.can_omit()
}

#[derive(Clone, Debug)]
struct CanOmitOptionalParenthesesVisitor<'input> {
    max_priority: OperatorPriority,
    max_priority_count: u32,
    any_parenthesized_expressions: bool,
    last: Option<&'input Expr>,
    first: Option<&'input Expr>,
    source: &'input str,
}

impl<'input> CanOmitOptionalParenthesesVisitor<'input> {
    fn new(source: &'input str) -> Self {
        Self {
            source,
            max_priority: OperatorPriority::None,
            max_priority_count: 0,
            any_parenthesized_expressions: false,
            last: None,
            first: None,
        }
    }

    fn update_max_priority(&mut self, current_priority: OperatorPriority) {
        self.update_max_priority_with_count(current_priority, 1);
    }

    fn update_max_priority_with_count(&mut self, current_priority: OperatorPriority, count: u32) {
        match self.max_priority.cmp(&current_priority) {
            Ordering::Less => {
                self.max_priority_count = count;
                self.max_priority = current_priority;
            }
            Ordering::Equal => {
                self.max_priority_count += count;
            }
            Ordering::Greater => {}
        }
    }

    // Visits a subexpression, ignoring whether it is parenthesized or not
    fn visit_subexpression(&mut self, expr: &'input Expr) {
        match expr {
            Expr::Dict(_) | Expr::List(_) | Expr::Tuple(_) | Expr::Set(_) => {
                self.any_parenthesized_expressions = true;
                // The values are always parenthesized, don't visit.
                return;
            }
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) => {
                self.any_parenthesized_expressions = true;
                self.update_max_priority(OperatorPriority::Comprehension);
                return;
            }
            // It's impossible for a file smaller or equal to 4GB to contain more than 2^32 comparisons
            // because each comparison requires a left operand, and `n` `operands` and right sides.
            #[allow(clippy::cast_possible_truncation)]
            Expr::BoolOp(ast::ExprBoolOp {
                range: _,
                op: _,
                values,
            }) => self.update_max_priority_with_count(
                OperatorPriority::BooleanOperation,
                values.len().saturating_sub(1) as u32,
            ),
            Expr::BinOp(ast::ExprBinOp {
                op,
                left: _,
                right: _,
                range: _,
            }) => self.update_max_priority(OperatorPriority::from(*op)),

            Expr::IfExp(_) => {
                // + 1 for the if and one for the else
                self.update_max_priority_with_count(OperatorPriority::Conditional, 2);
            }

            // It's impossible for a file smaller or equal to 4GB to contain more than 2^32 comparisons
            // because each comparison requires a left operand, and `n` `operands` and right sides.
            #[allow(clippy::cast_possible_truncation)]
            Expr::Compare(ast::ExprCompare {
                range: _,
                left: _,
                ops,
                comparators: _,
            }) => {
                self.update_max_priority_with_count(OperatorPriority::Comparator, ops.len() as u32);
            }
            Expr::Call(ast::ExprCall {
                range: _,
                func,
                args: _,
                keywords: _,
            }) => {
                self.any_parenthesized_expressions = true;
                // Only walk the function, the arguments are always parenthesized
                self.visit_expr(func);
                self.last = Some(expr);
                return;
            }
            Expr::Subscript(_) => {
                // Don't walk the value. Splitting before the value looks weird.
                // Don't walk the slice, because the slice is always parenthesized.
                return;
            }
            Expr::UnaryOp(ast::ExprUnaryOp {
                range: _,
                op,
                operand: _,
            }) => {
                if op.is_invert() {
                    self.update_max_priority(OperatorPriority::BitwiseInversion);
                }
            }

            // `[a, b].test.test[300].dot`
            Expr::Attribute(ast::ExprAttribute {
                range: _,
                value,
                attr: _,
                ctx: _,
            }) => {
                if has_parentheses(value, self.source) {
                    self.update_max_priority(OperatorPriority::Attribute);
                }
            }

            Expr::NamedExpr(_)
            | Expr::GeneratorExp(_)
            | Expr::Lambda(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::FormattedValue(_)
            | Expr::JoinedStr(_)
            | Expr::Constant(_)
            | Expr::Starred(_)
            | Expr::Name(_)
            | Expr::Slice(_) => {}
            Expr::LineMagic(_) => todo!(),
        };

        walk_expr(self, expr);
    }

    fn can_omit(self) -> bool {
        if self.max_priority_count > 1 {
            false
        } else if self.max_priority == OperatorPriority::Attribute {
            true
        } else if !self.any_parenthesized_expressions {
            // Only use the more complex IR when there is any expression that we can possibly split by
            false
        } else {
            // Only use the layout if the first or last expression has parentheses of some sort.
            let first_parenthesized = self
                .first
                .map_or(false, |first| has_parentheses(first, self.source));
            let last_parenthesized = self
                .last
                .map_or(false, |last| has_parentheses(last, self.source));
            first_parenthesized || last_parenthesized
        }
    }
}

impl<'input> PreorderVisitor<'input> for CanOmitOptionalParenthesesVisitor<'input> {
    fn visit_expr(&mut self, expr: &'input Expr) {
        self.last = Some(expr);

        // Rule only applies for non-parenthesized expressions.
        if is_expression_parenthesized(AnyNodeRef::from(expr), self.source) {
            self.any_parenthesized_expressions = true;
        } else {
            self.visit_subexpression(expr);
        }

        if self.first.is_none() {
            self.first = Some(expr);
        }
    }
}

fn has_parentheses(expr: &Expr, source: &str) -> bool {
    has_own_parentheses(expr) || is_expression_parenthesized(AnyNodeRef::from(expr), source)
}

pub(crate) const fn has_own_parentheses(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Dict(_)
            | Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Call(_)
            | Expr::Subscript(_)
    )
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
enum OperatorPriority {
    None,
    Attribute,
    Comparator,
    Exponential,
    BitwiseInversion,
    Multiplicative,
    Additive,
    Shift,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    // TODO(micha)
    #[allow(unused)]
    String,
    BooleanOperation,
    Conditional,
    Comprehension,
}

impl From<ast::Operator> for OperatorPriority {
    fn from(value: Operator) -> Self {
        match value {
            Operator::Add | Operator::Sub => OperatorPriority::Additive,
            Operator::Mult
            | Operator::MatMult
            | Operator::Div
            | Operator::Mod
            | Operator::FloorDiv => OperatorPriority::Multiplicative,
            Operator::Pow => OperatorPriority::Exponential,
            Operator::LShift | Operator::RShift => OperatorPriority::Shift,
            Operator::BitOr => OperatorPriority::BitwiseOr,
            Operator::BitXor => OperatorPriority::BitwiseXor,
            Operator::BitAnd => OperatorPriority::BitwiseAnd,
        }
    }
}
