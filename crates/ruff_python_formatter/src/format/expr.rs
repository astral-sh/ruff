#![allow(unused_variables, clippy::too_many_arguments)]

use rustpython_parser::ast::{Constant, ConversionFlag};

use ruff_formatter::prelude::*;
use ruff_formatter::{format_args, write};

use crate::context::ASTFormatContext;
use crate::cst::{
    Arguments, BoolOp, CmpOp, Comprehension, Expr, ExprKind, Keyword, Operator, OperatorKind,
    SliceIndex, SliceIndexKind, UnaryOp, UnaryOpKind,
};
use crate::format::builders::literal;
use crate::format::comments::{dangling_comments, end_of_line_comments, leading_comments};
use crate::format::helpers::{is_self_closing, is_simple_power, is_simple_slice};
use crate::format::numbers::{complex_literal, float_literal, int_literal};
use crate::format::strings::string_literal;
use crate::shared_traits::AsFormat;
use crate::trivia::{Parenthesize, TriviaKind};

pub struct FormatExpr<'a> {
    item: &'a Expr,
}

fn format_starred(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: &Expr,
) -> FormatResult<()> {
    write!(f, [text("*"), value.format()])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_name(f: &mut Formatter<ASTFormatContext>, expr: &Expr, _id: &str) -> FormatResult<()> {
    write!(f, [literal(expr.range(), ContainsNewlines::No)])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_subscript(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: &Expr,
    slice: &Expr,
) -> FormatResult<()> {
    write!(f, [value.format()])?;
    write!(f, [text("[")])?;
    write!(
        f,
        [group(&format_args![soft_block_indent(&format_with(|f| {
            write!(f, [slice.format()])?;

            // Apply any dangling comments.
            for trivia in &expr.trivia {
                if trivia.relationship.is_dangling() {
                    if let TriviaKind::OwnLineComment(range) = trivia.kind {
                        write!(f, [expand_parent()])?;
                        write!(f, [hard_line_break()])?;
                        write!(f, [literal(range, ContainsNewlines::No)])?;
                    }
                }
            }

            Ok(())
        }))])]
    )?;
    write!(f, [text("]")])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_tuple(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    elts: &[Expr],
) -> FormatResult<()> {
    // If we're already parenthesized, avoid adding any "mandatory" parentheses.
    // TODO(charlie): We also need to parenthesize tuples on the right-hand side of an
    // assignment if the target is exploded. And sometimes the tuple gets exploded, like
    // if the LHS is an exploded list? Lots of edge cases here.
    if elts.len() == 1 {
        write!(
            f,
            [group(&format_args![soft_block_indent(&format_with(|f| {
                write!(f, [elts[0].format()])?;
                write!(f, [text(",")])?;
                Ok(())
            }))])]
        )?;
    } else if !elts.is_empty() {
        write!(
            f,
            [group(&format_with(
                |f: &mut Formatter<ASTFormatContext>| {
                    if expr.parentheses.is_if_expanded() {
                        write!(f, [if_group_breaks(&text("("))])?;
                    }
                    if matches!(
                        expr.parentheses,
                        Parenthesize::IfExpanded | Parenthesize::Always
                    ) {
                        write!(
                            f,
                            [soft_block_indent(&format_with(
                                |f: &mut Formatter<ASTFormatContext>| {
                                    let magic_trailing_comma = expr
                                        .trivia
                                        .iter()
                                        .any(|c| c.kind.is_magic_trailing_comma());
                                    let is_unbroken =
                                        !f.context().locator().contains_line_break(expr.range());
                                    if magic_trailing_comma {
                                        write!(f, [expand_parent()])?;
                                    }
                                    for (i, elt) in elts.iter().enumerate() {
                                        write!(f, [elt.format()])?;
                                        if i < elts.len() - 1 {
                                            write!(f, [text(",")])?;
                                            write!(f, [soft_line_break_or_space()])?;
                                        } else {
                                            if magic_trailing_comma || is_unbroken {
                                                write!(f, [if_group_breaks(&text(","))])?;
                                            }
                                        }
                                    }
                                    Ok(())
                                }
                            ))]
                        )?;
                    } else {
                        let magic_trailing_comma =
                            expr.trivia.iter().any(|c| c.kind.is_magic_trailing_comma());
                        let is_unbroken = !f.context().locator().contains_line_break(expr.range());
                        if magic_trailing_comma {
                            write!(f, [expand_parent()])?;
                        }
                        for (i, elt) in elts.iter().enumerate() {
                            write!(f, [elt.format()])?;
                            if i < elts.len() - 1 {
                                write!(f, [text(",")])?;
                                write!(f, [soft_line_break_or_space()])?;
                            } else {
                                if magic_trailing_comma || is_unbroken {
                                    write!(f, [if_group_breaks(&text(","))])?;
                                }
                            }
                        }
                    }
                    if expr.parentheses.is_if_expanded() {
                        write!(f, [if_group_breaks(&text(")"))])?;
                    }
                    Ok(())
                }
            ))]
        )?;
    }
    Ok(())
}

fn format_slice(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    lower: &SliceIndex,
    upper: &SliceIndex,
    step: Option<&SliceIndex>,
) -> FormatResult<()> {
    // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#slices
    let lower_is_simple = if let SliceIndexKind::Index { value } = &lower.node {
        is_simple_slice(value)
    } else {
        true
    };
    let upper_is_simple = if let SliceIndexKind::Index { value } = &upper.node {
        is_simple_slice(value)
    } else {
        true
    };
    let step_is_simple = step.map_or(true, |step| {
        if let SliceIndexKind::Index { value } = &step.node {
            is_simple_slice(value)
        } else {
            true
        }
    });
    let is_simple = lower_is_simple && upper_is_simple && step_is_simple;

    write!(
        f,
        [group(&format_with(|f| {
            if let SliceIndexKind::Index { value } = &lower.node {
                write!(f, [value.format()])?;
            }

            write!(f, [dangling_comments(lower)])?;

            if matches!(lower.node, SliceIndexKind::Index { .. }) {
                if !is_simple {
                    write!(f, [space()])?;
                }
            }
            write!(f, [text(":")])?;
            write!(f, [end_of_line_comments(lower)])?;

            if let SliceIndexKind::Index { value } = &upper.node {
                if !is_simple {
                    write!(f, [space()])?;
                }
                write!(f, [if_group_breaks(&soft_line_break())])?;
                write!(f, [value.format()])?;
            }

            write!(f, [dangling_comments(upper)])?;
            write!(f, [end_of_line_comments(upper)])?;

            if let Some(step) = step {
                if matches!(upper.node, SliceIndexKind::Index { .. }) {
                    if !is_simple {
                        write!(f, [space()])?;
                    }
                }
                write!(f, [text(":")])?;

                if let SliceIndexKind::Index { value } = &step.node {
                    if !is_simple {
                        write!(f, [space()])?;
                    }
                    write!(f, [if_group_breaks(&soft_line_break())])?;
                    write!(f, [value.format()])?;
                }

                write!(f, [dangling_comments(step)])?;
                write!(f, [end_of_line_comments(step)])?;
            }
            Ok(())
        }))]
    )?;

    write!(f, [end_of_line_comments(expr)])?;

    Ok(())
}

fn format_formatted_value(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: &Expr,
    _conversion: ConversionFlag,
    format_spec: Option<&Expr>,
) -> FormatResult<()> {
    write!(f, [text("!")])?;
    write!(f, [value.format()])?;
    if let Some(format_spec) = format_spec {
        write!(f, [text(":")])?;
        write!(f, [format_spec.format()])?;
    }
    Ok(())
}

fn format_list(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    elts: &[Expr],
) -> FormatResult<()> {
    write!(f, [text("[")])?;
    if !elts.is_empty() {
        let magic_trailing_comma = expr.trivia.iter().any(|c| c.kind.is_magic_trailing_comma());
        write!(
            f,
            [group(&format_args![soft_block_indent(&format_with(|f| {
                if magic_trailing_comma {
                    write!(f, [expand_parent()])?;
                }
                for (i, elt) in elts.iter().enumerate() {
                    write!(f, [elt.format()])?;
                    if i < elts.len() - 1 {
                        write!(f, [text(",")])?;
                        write!(f, [soft_line_break_or_space()])?;
                    } else {
                        write!(f, [if_group_breaks(&text(","))])?;
                    }
                }
                Ok(())
            }))])]
        )?;
    }
    write!(f, [text("]")])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_set(f: &mut Formatter<ASTFormatContext>, expr: &Expr, elts: &[Expr]) -> FormatResult<()> {
    if elts.is_empty() {
        write!(f, [text("set()")])?;
        Ok(())
    } else {
        write!(f, [text("{")])?;
        if !elts.is_empty() {
            let magic_trailing_comma = expr.trivia.iter().any(|c| c.kind.is_magic_trailing_comma());
            write!(
                f,
                [group(&format_args![soft_block_indent(&format_with(|f| {
                    if magic_trailing_comma {
                        write!(f, [expand_parent()])?;
                    }
                    for (i, elt) in elts.iter().enumerate() {
                        write!(f, [group(&format_args![elt.format()])])?;
                        if i < elts.len() - 1 {
                            write!(f, [text(",")])?;
                            write!(f, [soft_line_break_or_space()])?;
                        } else {
                            if magic_trailing_comma {
                                write!(f, [if_group_breaks(&text(","))])?;
                            }
                        }
                    }
                    Ok(())
                }))])]
            )?;
        }
        write!(f, [text("}")])?;
        Ok(())
    }
}

fn format_call(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> FormatResult<()> {
    write!(f, [func.format()])?;
    if args.is_empty() && keywords.is_empty() {
        write!(f, [text("(")])?;
        write!(f, [text(")")])?;
        write!(f, [end_of_line_comments(expr)])?;
    } else {
        write!(f, [text("(")])?;
        write!(f, [end_of_line_comments(expr)])?;

        let magic_trailing_comma = expr.trivia.iter().any(|c| c.kind.is_magic_trailing_comma());
        write!(
            f,
            [group(&format_args![soft_block_indent(&format_with(|f| {
                if magic_trailing_comma {
                    write!(f, [expand_parent()])?;
                }

                for (i, arg) in args.iter().enumerate() {
                    write!(f, [group(&format_args![arg.format()])])?;
                    if i < args.len() - 1 || !keywords.is_empty() {
                        write!(f, [text(",")])?;
                        write!(f, [soft_line_break_or_space()])?;
                    } else {
                        if magic_trailing_comma || (args.len() + keywords.len() > 1) {
                            write!(f, [if_group_breaks(&text(","))])?;
                        }
                    }
                }
                for (i, keyword) in keywords.iter().enumerate() {
                    write!(
                        f,
                        [group(&format_args![&format_with(|f| {
                            write!(f, [keyword.format()])?;
                            Ok(())
                        })])]
                    )?;
                    if i < keywords.len() - 1 {
                        write!(f, [text(",")])?;
                        write!(f, [soft_line_break_or_space()])?;
                    } else {
                        if magic_trailing_comma || (args.len() + keywords.len() > 1) {
                            write!(f, [if_group_breaks(&text(","))])?;
                        }
                    }
                }

                write!(f, [dangling_comments(expr)])?;

                Ok(())
            }))])]
        )?;
        write!(f, [text(")")])?;
    }
    Ok(())
}

fn format_list_comp(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) -> FormatResult<()> {
    write!(f, [text("[")])?;
    write!(
        f,
        [group(&format_args![soft_block_indent(&format_with(|f| {
            write!(f, [elt.format()])?;
            for generator in generators {
                write!(f, [generator.format()])?;
            }
            Ok(())
        }))])]
    )?;
    write!(f, [text("]")])?;
    Ok(())
}

fn format_set_comp(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) -> FormatResult<()> {
    write!(f, [text("{")])?;
    write!(
        f,
        [group(&format_args![soft_block_indent(&format_with(|f| {
            write!(f, [elt.format()])?;
            for generator in generators {
                write!(f, [generator.format()])?;
            }
            Ok(())
        }))])]
    )?;
    write!(f, [text("}")])?;
    Ok(())
}

fn format_dict_comp(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    key: &Expr,
    value: &Expr,
    generators: &[Comprehension],
) -> FormatResult<()> {
    write!(f, [text("{")])?;
    write!(
        f,
        [group(&format_args![soft_block_indent(&format_with(|f| {
            write!(f, [key.format()])?;
            write!(f, [text(":")])?;
            write!(f, [space()])?;
            write!(f, [value.format()])?;
            for generator in generators {
                write!(f, [generator.format()])?;
            }
            Ok(())
        }))])]
    )?;
    write!(f, [text("}")])?;
    Ok(())
}

fn format_generator_exp(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    elt: &Expr,
    generators: &[Comprehension],
) -> FormatResult<()> {
    write!(
        f,
        [group(&format_args![soft_block_indent(&format_with(|f| {
            write!(f, [elt.format()])?;
            for generator in generators {
                write!(f, [generator.format()])?;
            }
            Ok(())
        }))])]
    )?;
    Ok(())
}

fn format_await(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: &Expr,
) -> FormatResult<()> {
    write!(f, [text("await")])?;
    write!(f, [space()])?;
    if is_self_closing(value) {
        write!(f, [group(&format_args![value.format()])])?;
    } else {
        write!(
            f,
            [group(&format_args![
                if_group_breaks(&text("(")),
                soft_block_indent(&format_args![value.format()]),
                if_group_breaks(&text(")")),
            ])]
        )?;
    }
    Ok(())
}

fn format_yield(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: Option<&Expr>,
) -> FormatResult<()> {
    write!(f, [text("yield")])?;
    if let Some(value) = value {
        write!(f, [space()])?;
        if is_self_closing(value) {
            write!(f, [group(&format_args![value.format()])])?;
        } else {
            write!(
                f,
                [group(&format_args![
                    if_group_breaks(&text("(")),
                    soft_block_indent(&format_args![value.format()]),
                    if_group_breaks(&text(")")),
                ])]
            )?;
        }
    }
    Ok(())
}

fn format_yield_from(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: &Expr,
) -> FormatResult<()> {
    write!(
        f,
        [group(&format_args![soft_block_indent(&format_with(|f| {
            write!(f, [text("yield")])?;
            write!(f, [space()])?;
            write!(f, [text("from")])?;
            write!(f, [space()])?;
            if is_self_closing(value) {
                write!(f, [value.format()])?;
            } else {
                write!(
                    f,
                    [group(&format_args![
                        if_group_breaks(&text("(")),
                        soft_block_indent(&format_args![value.format()]),
                        if_group_breaks(&text(")")),
                    ])]
                )?;
            }
            Ok(())
        })),])]
    )?;
    Ok(())
}

fn format_compare(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) -> FormatResult<()> {
    write!(f, [group(&format_args![left.format()])])?;
    for (i, op) in ops.iter().enumerate() {
        write!(f, [soft_line_break_or_space()])?;
        write!(f, [op.format()])?;
        write!(f, [space()])?;
        write!(f, [group(&format_args![comparators[i].format()])])?;
    }

    write!(f, [end_of_line_comments(expr)])?;

    Ok(())
}

fn format_joined_str(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    _values: &[Expr],
) -> FormatResult<()> {
    write!(f, [literal(expr.range(), ContainsNewlines::Detect)])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_constant(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    constant: &Constant,
    _kind: Option<&str>,
) -> FormatResult<()> {
    match constant {
        Constant::Ellipsis => write!(f, [text("...")])?,
        Constant::None => write!(f, [text("None")])?,
        Constant::Bool(value) => {
            if *value {
                write!(f, [text("True")])?;
            } else {
                write!(f, [text("False")])?;
            }
        }
        Constant::Int(_) => write!(f, [int_literal(expr.range())])?,
        Constant::Float(_) => write!(f, [float_literal(expr.range())])?,
        Constant::Str(_) => write!(f, [string_literal(expr)])?,
        Constant::Bytes(_) => write!(f, [string_literal(expr)])?,
        Constant::Complex { .. } => write!(f, [complex_literal(expr.range())])?,
        Constant::Tuple(_) => unreachable!("Constant::Tuple should be handled by format_tuple"),
    }
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_dict(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    keys: &[Option<Expr>],
    values: &[Expr],
) -> FormatResult<()> {
    write!(f, [text("{")])?;
    if !keys.is_empty() {
        let magic_trailing_comma = expr.trivia.iter().any(|c| c.kind.is_magic_trailing_comma());
        write!(
            f,
            [soft_block_indent(&format_with(|f| {
                if magic_trailing_comma {
                    write!(f, [expand_parent()])?;
                }
                for (i, (k, v)) in keys.iter().zip(values).enumerate() {
                    if let Some(k) = k {
                        write!(f, [k.format()])?;
                        write!(f, [text(":")])?;
                        write!(f, [space()])?;
                        if is_self_closing(v) {
                            write!(f, [v.format()])?;
                        } else {
                            write!(
                                f,
                                [group(&format_args![
                                    if_group_breaks(&text("(")),
                                    soft_block_indent(&format_args![v.format()]),
                                    if_group_breaks(&text(")")),
                                ])]
                            )?;
                        }
                    } else {
                        write!(f, [text("**")])?;
                        if is_self_closing(v) {
                            write!(f, [v.format()])?;
                        } else {
                            write!(
                                f,
                                [group(&format_args![
                                    if_group_breaks(&text("(")),
                                    soft_block_indent(&format_args![v.format()]),
                                    if_group_breaks(&text(")")),
                                ])]
                            )?;
                        }
                    }
                    if i < keys.len() - 1 {
                        write!(f, [text(",")])?;
                        write!(f, [soft_line_break_or_space()])?;
                    } else {
                        write!(f, [if_group_breaks(&text(","))])?;
                    }
                }
                Ok(())
            }))]
        )?;
    }
    write!(f, [text("}")])?;
    Ok(())
}

fn format_attribute(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    value: &Expr,
    attr: &str,
) -> FormatResult<()> {
    write!(f, [value.format()])?;
    write!(f, [text(".")])?;
    write!(f, [dynamic_text(attr, None)])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_named_expr(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    target: &Expr,
    value: &Expr,
) -> FormatResult<()> {
    write!(f, [target.format()])?;
    write!(f, [text(":=")])?;
    write!(f, [space()])?;
    write!(f, [group(&format_args![value.format()])])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_bool_op(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    ops: &[BoolOp],
    values: &[Expr],
) -> FormatResult<()> {
    write!(f, [group(&format_args![values[0].format()])])?;
    for (op, value) in ops.iter().zip(&values[1..]) {
        write!(f, [soft_line_break_or_space()])?;
        write!(f, [op.format()])?;
        write!(f, [space()])?;
        write!(f, [group(&format_args![value.format()])])?;
    }
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_bin_op(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    left: &Expr,
    op: &Operator,
    right: &Expr,
) -> FormatResult<()> {
    // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#line-breaks-binary-operators
    let is_simple =
        matches!(op.node, OperatorKind::Pow) && is_simple_power(left) && is_simple_power(right);
    write!(f, [left.format()])?;
    if !is_simple {
        write!(f, [soft_line_break_or_space()])?;
    }
    write!(f, [op.format()])?;
    if !is_simple {
        write!(f, [space()])?;
    }
    write!(f, [group(&right.format())])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_unary_op(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    op: &UnaryOp,
    operand: &Expr,
) -> FormatResult<()> {
    write!(f, [op.format()])?;
    // TODO(charlie): Do this in the normalization pass.
    if !matches!(op.node, UnaryOpKind::Not)
        && matches!(
            operand.node,
            ExprKind::BoolOp { .. } | ExprKind::Compare { .. } | ExprKind::BinOp { .. }
        )
    {
        let parenthesized = operand.parentheses.is_always();
        if !parenthesized {
            write!(f, [text("(")])?;
        }
        write!(f, [operand.format()])?;
        if !parenthesized {
            write!(f, [text(")")])?;
        }
    } else {
        write!(f, [operand.format()])?;
    }
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_lambda(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    args: &Arguments,
    body: &Expr,
) -> FormatResult<()> {
    write!(f, [text("lambda")])?;
    if !args.args.is_empty() || args.kwarg.is_some() || args.vararg.is_some() {
        write!(f, [space()])?;
        write!(f, [args.format()])?;
    }
    write!(f, [text(":")])?;
    write!(f, [space()])?;
    write!(f, [body.format()])?;
    write!(f, [end_of_line_comments(expr)])?;
    Ok(())
}

fn format_if_exp(
    f: &mut Formatter<ASTFormatContext>,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) -> FormatResult<()> {
    write!(f, [group(&format_args![body.format()])])?;
    write!(f, [soft_line_break_or_space()])?;
    write!(f, [text("if")])?;
    write!(f, [space()])?;
    write!(f, [group(&format_args![test.format()])])?;
    write!(f, [soft_line_break_or_space()])?;
    write!(f, [text("else")])?;
    write!(f, [space()])?;
    write!(f, [group(&format_args![orelse.format()])])?;
    Ok(())
}

impl Format<ASTFormatContext<'_>> for FormatExpr<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        if self.item.parentheses.is_always() {
            write!(f, [text("(")])?;
        }

        write!(f, [leading_comments(self.item)])?;

        match &self.item.node {
            ExprKind::BoolOp { ops, values } => format_bool_op(f, self.item, ops, values),
            ExprKind::NamedExpr { target, value } => format_named_expr(f, self.item, target, value),
            ExprKind::BinOp { left, op, right } => format_bin_op(f, self.item, left, op, right),
            ExprKind::UnaryOp { op, operand } => format_unary_op(f, self.item, op, operand),
            ExprKind::Lambda { args, body } => format_lambda(f, self.item, args, body),
            ExprKind::IfExp { test, body, orelse } => {
                format_if_exp(f, self.item, test, body, orelse)
            }
            ExprKind::Dict { keys, values } => format_dict(f, self.item, keys, values),
            ExprKind::Set { elts, .. } => format_set(f, self.item, elts),
            ExprKind::ListComp { elt, generators } => {
                format_list_comp(f, self.item, elt, generators)
            }
            ExprKind::SetComp { elt, generators } => format_set_comp(f, self.item, elt, generators),
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => format_dict_comp(f, self.item, key, value, generators),
            ExprKind::GeneratorExp { elt, generators } => {
                format_generator_exp(f, self.item, elt, generators)
            }
            ExprKind::Await { value } => format_await(f, self.item, value),
            ExprKind::Yield { value } => format_yield(f, self.item, value.as_deref()),
            ExprKind::YieldFrom { value } => format_yield_from(f, self.item, value),
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => format_compare(f, self.item, left, ops, comparators),
            ExprKind::Call {
                func,
                args,
                keywords,
            } => format_call(f, self.item, func, args, keywords),
            ExprKind::JoinedStr { values } => format_joined_str(f, self.item, values),
            ExprKind::Constant { value, kind } => {
                format_constant(f, self.item, value, kind.as_deref())
            }
            ExprKind::Attribute { value, attr, .. } => format_attribute(f, self.item, value, attr),
            ExprKind::Subscript { value, slice, .. } => {
                format_subscript(f, self.item, value, slice)
            }
            ExprKind::Starred { value, .. } => format_starred(f, self.item, value),
            ExprKind::Name { id, .. } => format_name(f, self.item, id),
            ExprKind::List { elts, .. } => format_list(f, self.item, elts),
            ExprKind::Tuple { elts, .. } => format_tuple(f, self.item, elts),
            ExprKind::Slice { lower, upper, step } => {
                format_slice(f, self.item, lower, upper, step.as_ref())
            }
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => format_formatted_value(f, self.item, value, *conversion, format_spec.as_deref()),
        }?;

        // Any trailing comments come on the lines after.
        for trivia in &self.item.trivia {
            if trivia.relationship.is_trailing() {
                if let TriviaKind::OwnLineComment(range) = trivia.kind {
                    write!(f, [expand_parent()])?;
                    write!(f, [literal(range, ContainsNewlines::No)])?;
                    write!(f, [hard_line_break()])?;
                }
            }
        }

        if self.item.parentheses.is_always() {
            write!(f, [text(")")])?;
        }

        Ok(())
    }
}

impl AsFormat<ASTFormatContext<'_>> for Expr {
    type Format<'a> = FormatExpr<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatExpr { item: self }
    }
}
