use crate::context::NodeLevel;
use crate::expression::parentheses::{NeedsParentheses, Parentheses, Parenthesize};
use crate::prelude::*;
use ruff_formatter::{
    format_args, write, FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions,
};
use rustpython_parser::ast::Expr;

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
        let parentheses = item.needs_parentheses(self.parenthesize, f.context().contents());

        let format_expr = format_with(|f| match item {
            Expr::BoolOp(expr) => expr.format().fmt(f),
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
            Expr::Compare(expr) => expr.format().fmt(f),
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
        });

        let saved_level = f.context().node_level();
        f.context_mut().set_node_level(NodeLevel::Expression);

        let result = match parentheses {
            Parentheses::Always => {
                write!(
                    f,
                    [group(&format_args![
                        text("("),
                        soft_block_indent(&format_expr),
                        text(")")
                    ])]
                )
            }
            // Add optional parentheses. Ignore if the item renders parentheses itself.
            Parentheses::Optional => {
                write!(
                    f,
                    [group(&format_args![
                        if_group_breaks(&text("(")),
                        soft_block_indent(&format_expr),
                        if_group_breaks(&text(")"))
                    ])]
                )
            }
            Parentheses::Custom | Parentheses::Never => Format::fmt(&format_expr, f),
        };

        f.context_mut().set_node_level(saved_level);

        result
    }
}

impl NeedsParentheses for Expr {
    fn needs_parentheses(&self, parenthesize: Parenthesize, source: &str) -> Parentheses {
        match self {
            Expr::BoolOp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::NamedExpr(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::BinOp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::UnaryOp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Lambda(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::IfExp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Dict(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Set(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::ListComp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::SetComp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::DictComp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::GeneratorExp(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Await(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Yield(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::YieldFrom(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Compare(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Call(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::FormattedValue(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::JoinedStr(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Constant(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Attribute(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Subscript(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Starred(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Name(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::List(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Tuple(expr) => expr.needs_parentheses(parenthesize, source),
            Expr::Slice(expr) => expr.needs_parentheses(parenthesize, source),
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
