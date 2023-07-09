use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Operator};
use std::cmp::Ordering;

use crate::builders::optional_parentheses;
use ruff_formatter::{
    format_args, FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions,
};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::visitor::preorder::{walk_expr, PreorderVisitor};

use crate::comments::Comments;
use crate::context::NodeLevel;
use crate::expression::expr_tuple::TupleParentheses;
use crate::expression::parentheses::{
    is_expression_parenthesized, parenthesized, NeedsParentheses, Parentheses, Parenthesize,
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
        let parentheses = item.needs_parentheses(
            self.parenthesize,
            f.context().contents(),
            f.context().comments(),
        );

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
                    let saved_level = f.context().node_level();

                    let parens_id = f.group_id("optional_parentheses");

                    f.context_mut()
                        .set_node_level(NodeLevel::Expression(Some(parens_id)));

                    let result = group(&format_args![
                        if_group_breaks(&text("(")),
                        indent_if_group_breaks(
                            &format_args![soft_line_break(), format_expr],
                            parens_id
                        ),
                        soft_line_break(),
                        if_group_breaks(&text(")"))
                    ])
                    .with_group_id(Some(parens_id))
                    .fmt(f);

                    f.context_mut().set_node_level(saved_level);

                    result
                } else {
                    optional_parentheses(&format_expr).fmt(f)
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
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match self {
            Expr::BoolOp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::NamedExpr(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::BinOp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::UnaryOp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Lambda(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::IfExp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Dict(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Set(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::ListComp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::SetComp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::DictComp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::GeneratorExp(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Await(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Yield(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::YieldFrom(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Compare(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Call(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::FormattedValue(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::JoinedStr(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Constant(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Attribute(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Subscript(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Starred(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Name(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::List(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Tuple(expr) => expr.needs_parentheses(parenthesize, source, comments),
            Expr::Slice(expr) => expr.needs_parentheses(parenthesize, source, comments),
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
fn can_omit_optional_parentheses(expr: &Expr, context: &PyFormatContext) -> bool {
    let mut visitor = MaxOperatorPriorityVisitor::new(context.contents());

    visitor.visit_subexpression(expr);

    let (max_operator_priority, operation_count, any_parenthesized_expression) = visitor.finish();

    if operation_count > 1 {
        false
    } else if max_operator_priority == OperatorPriority::Attribute {
        true
    } else {
        // Only use the more complex IR when there is any expression that we can possibly split by
        any_parenthesized_expression
    }
}

#[derive(Clone, Debug)]
struct MaxOperatorPriorityVisitor<'input> {
    max_priority: OperatorPriority,
    max_priority_count: u32,
    any_parenthesized_expressions: bool,
    source: &'input str,
}

impl<'input> MaxOperatorPriorityVisitor<'input> {
    fn new(source: &'input str) -> Self {
        Self {
            source,
            max_priority: OperatorPriority::None,
            max_priority_count: 0,
            any_parenthesized_expressions: false,
        }
    }

    fn update_max_priority(&mut self, current_priority: OperatorPriority) {
        match self.max_priority.cmp(&current_priority) {
            Ordering::Less => {
                self.max_priority_count = 1;
                self.max_priority = current_priority;
            }
            Ordering::Equal => {
                self.max_priority_count += 1;
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
            Expr::BoolOp(ast::ExprBoolOp {
                range: _,
                op: _,
                values: _,
            }) => self.update_max_priority(OperatorPriority::BooleanOperation),
            Expr::BinOp(ast::ExprBinOp {
                op,
                left: _,
                right: _,
                range: _,
            }) => self.update_max_priority(OperatorPriority::from(*op)),

            Expr::IfExp(ast::ExprIfExp {
                range: _,
                test,
                body: _,
                orelse: _,
            }) => {
                self.update_max_priority(OperatorPriority::Conditional);

                // Nested if else expressions are always parenthesized. Ignore parentheses in this case
                if let Expr::IfExp(_) = test.as_ref() {
                    self.update_max_priority(OperatorPriority::Conditional);
                }
            }

            Expr::Compare(_) => self.update_max_priority(OperatorPriority::Comparator),
            Expr::Call(ast::ExprCall {
                range: _,
                func,
                args: _,
                keywords: _,
            }) => {
                self.any_parenthesized_expressions = true;
                // Only walk the function, the arguments are always parenthesized
                self.visit_expr(func);
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
                if has_parentheses(&value, self.source) {
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

    fn finish(self) -> (OperatorPriority, u32, bool) {
        (
            self.max_priority,
            self.max_priority_count,
            self.any_parenthesized_expressions,
        )
    }
}

impl<'input> PreorderVisitor<'input> for MaxOperatorPriorityVisitor<'input> {
    fn visit_expr(&mut self, expr: &'input Expr) {
        // Rule only applies for non-parenthesized expressions.
        if is_expression_parenthesized(AnyNodeRef::from(expr), self.source) {
            self.any_parenthesized_expressions = true;
        } else {
            self.visit_subexpression(expr);
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
