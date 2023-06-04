use crate::context::NodeLevel;
use crate::expression::expr_bin_op::should_binary_break_right_side_first;
use crate::prelude::*;
use crate::trivia::{
    find_first_non_trivia_character_after, find_first_non_trivia_character_before,
};
use ruff_formatter::{
    format_args, write, FormatOwnedWithRule, FormatRefWithRule, FormatRule, FormatRuleWithOptions,
};
use rustpython_parser::ast::{Expr, Ranged};
use std::fmt::Debug;

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
        let parenthesize = !self.parenthesize.is_if_breaks()
            && is_expression_parenthesized(item, f.context().contents());

        let saved_level = f.context().node_level();
        f.context_mut().set_node_level(NodeLevel::Expression);

        // `Optional` or `Preserve` and expression has parentheses in source code.
        let parentheses = if parenthesize {
            Parentheses::Always
        }
        // `Optional` or `IfBreaks`: Add parentheses if the expression doesn't fit on a line
        else if !self.parenthesize.is_preserve() {
            Parentheses::Optional
        } else {
            //`Preserve` and expression has no parentheses in the source code
            Parentheses::Never
        };

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
            Parentheses::Optional if !has_own_parentheses(item) => {
                write!(
                    f,
                    [group(&format_args![
                        if_group_breaks(&text("(")),
                        soft_block_indent(&format_expr),
                        if_group_breaks(&text(")"))
                    ])]
                )
            }
            Parentheses::Never | Parentheses::Optional => Format::fmt(&format_expr, f),
        };

        f.context_mut().set_node_level(saved_level);

        result
    }
}

/// Configures if the expression should be parenthesized.
#[derive(Copy, Clone, Debug, Default)]
pub enum Parenthesize {
    /// Parenthesize the expression if it has parenthesis in the source.
    #[default]
    Preserve,

    /// Parenthesizes the expression if it doesn't fit on a line OR if the expression is parenthesized in the source code.
    Optional,

    /// Parenthesizes the expression only if it doesn't fit on a line.
    IfBreaks,
}

impl Parenthesize {
    const fn is_if_breaks(self) -> bool {
        matches!(self, Parenthesize::IfBreaks)
    }

    const fn is_preserve(self) -> bool {
        matches!(self, Parenthesize::Preserve)
    }
}

/// Whether it is necessary to add parentheses around an expression.
/// This is different from [`Parenthesize`] in that it is the resolved representation: It takes into account
/// whether there are parentheses in the source code or not.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Parentheses {
    /// Always create parentheses
    Always,

    /// Only add parentheses when necessary because the expression breaks over multiple lines.
    Optional,

    /// Never add parentheses
    Never,
}

fn is_expression_parenthesized(expr: &Expr, contents: &str) -> bool {
    // Search backwards to avoid ambiguity with `(a, )` and because it's faster
    matches!(
        find_first_non_trivia_character_after(expr.end(), contents),
        Some((_, ')'))
    )
    // Search forwards to confirm that this is not a nested expression `(5 + d * 3)`
    && matches!(
        find_first_non_trivia_character_before(expr.start(), contents),
        Some((_, '('))
    )
}

/// Returns `true` if `expr` adds its own parentheses.
fn has_own_parentheses(expr: &Expr) -> bool {
    match expr {
        Expr::Tuple(_)
        | Expr::List(_)
        | Expr::Set(_)
        | Expr::Dict(_)
        | Expr::ListComp(_)
        | Expr::SetComp(_)
        | Expr::DictComp(_)
        | Expr::GeneratorExp(_)
        | Expr::Call(_) => true,
        // Handles parentheses on its own.
        Expr::BinOp(binary) => should_binary_break_right_side_first(binary),
        _ => false,
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
