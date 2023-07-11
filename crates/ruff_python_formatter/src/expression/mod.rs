use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Operator};
use std::cmp::Ordering;

use crate::builders::parenthesize_if_expands;
use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::visitor::preorder::{walk_expr, PreorderVisitor};

use crate::context::NodeLevel;
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{
    is_expression_parenthesized, optional_parentheses, parenthesized, NeedsParentheses,
    Parentheses, Parenthesize,
};
use crate::expression::string::StringLayout;
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
pub(crate) mod parentheses;
pub(crate) mod string;

#[derive(Default)]
pub struct FormatExpr {
    parenthesize: Parenthesize,
}

impl FormatRuleWithOptions<Expr, PyFormatContext<'_>> for FormatExpr {
    type Options = Parenthesize;

    fn with_options(mut self, options: Self::Options) -> Self {
        self.parenthesize = options;
        self
    }
}

impl FormatRule<Expr, PyFormatContext<'_>> for FormatExpr {
    fn fmt(&self, item: &Expr, f: &mut PyFormatter) -> FormatResult<()> {
        let parentheses = item.needs_parentheses(self.parenthesize, f.context());

        let format_expr = format_with(|f| match item {
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
            Expr::Constant(expr) => expr
                .format()
                .with_options(StringLayout::Default(Some(parentheses)))
                .fmt(f),
            Expr::Attribute(expr) => expr.format().fmt(f),
            Expr::Subscript(expr) => expr.format().fmt(f),
            Expr::Starred(expr) => expr.format().fmt(f),
            Expr::Name(expr) => expr.format().fmt(f),
            Expr::List(expr) => expr.format().fmt(f),
            Expr::Tuple(expr) => expr
                .format()
                .with_options(TupleParentheses::Expr(parentheses))
                .fmt(f),
            Expr::Slice(expr) => expr.format().fmt(f),
        });

        let result = match parentheses {
            Parentheses::Always => parenthesized("(", &format_expr, ")").fmt(f),
            // Add optional parentheses. Ignore if the item renders parentheses itself.
            Parentheses::Optional => {
                if can_omit_optional_parentheses(item, f.context()) {
                    optional_parentheses(&format_expr).fmt(f)
                } else {
                    parenthesize_if_expands(&format_expr).fmt(f)
                }
            }
            Parentheses::Custom | Parentheses::Never => {
                let saved_level = f.context().node_level();

                let new_level = match saved_level {
                    NodeLevel::TopLevel | NodeLevel::CompoundStatement => {
                        NodeLevel::Expression(None)
                    }
                    level @ (NodeLevel::Expression(_) | NodeLevel::ParenthesizedExpression) => {
                        level
                    }
                };

                f.context_mut().set_node_level(new_level);

                let result = Format::fmt(&format_expr, f);
                f.context_mut().set_node_level(saved_level);
                result
            }
        };

        result
    }
}

impl NeedsParentheses for Expr {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        context: &PyFormatContext,
    ) -> Parentheses {
        match self {
            Expr::BoolOp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::NamedExpr(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::BinOp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::UnaryOp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Lambda(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::IfExp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Dict(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Set(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::ListComp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::SetComp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::DictComp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::GeneratorExp(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Await(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Yield(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::YieldFrom(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Compare(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Call(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::FormattedValue(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::JoinedStr(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Constant(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Attribute(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Subscript(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Starred(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Name(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::List(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Tuple(expr) => expr.needs_parentheses(parenthesize, context),
            Expr::Slice(expr) => expr.needs_parentheses(parenthesize, context),
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

            // `[a, b].test[300].dot`
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
    ) || is_expression_parenthesized(AnyNodeRef::from(expr), source)
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
