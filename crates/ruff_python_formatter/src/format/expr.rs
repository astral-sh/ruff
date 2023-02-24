#![allow(unused_variables, clippy::too_many_arguments)]

use rustpython_parser::ast::Constant;

use ruff_formatter::prelude::*;
use ruff_formatter::{format_args, write};
use ruff_text_size::TextSize;

use crate::builders::literal;
use crate::context::ASTFormatContext;
use crate::core::types::Range;
use crate::cst::{
    Arguments, Boolop, Cmpop, Comprehension, Expr, ExprKind, Keyword, Operator, SliceIndex,
    SliceIndexKind, Unaryop,
};
use crate::format::helpers::{is_self_closing, is_simple_power, is_simple_slice};
use crate::format::numbers::{complex_literal, float_literal, int_literal};
use crate::format::strings::string_literal;
use crate::shared_traits::AsFormat;
use crate::trivia::{Parenthesize, Relationship, TriviaKind};

pub struct FormatExpr<'a> {
    item: &'a Expr,
}

fn format_starred(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    value: &Expr,
) -> FormatResult<()> {
    write!(f, [text("*"), value.format()])?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_name(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    _id: &str,
) -> FormatResult<()> {
    write!(f, [literal(Range::from_located(expr))])?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_subscript(
    f: &mut Formatter<ASTFormatContext<'_>>,
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
                if matches!(trivia.relationship, Relationship::Dangling) {
                    if let TriviaKind::OwnLineComment(range) = trivia.kind {
                        write!(f, [expand_parent()])?;
                        write!(f, [hard_line_break()])?;
                        write!(f, [literal(range)])?;
                    }
                }
            }

            Ok(())
        }))])]
    )?;

    write!(f, [text("]")])?;

    Ok(())
}

fn format_tuple(
    f: &mut Formatter<ASTFormatContext<'_>>,
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
            [group(&format_with(|f| {
                if matches!(expr.parentheses, Parenthesize::IfExpanded) {
                    write!(f, [if_group_breaks(&text("("))])?;
                }
                if matches!(
                    expr.parentheses,
                    Parenthesize::IfExpanded | Parenthesize::Always
                ) {
                    write!(
                        f,
                        [soft_block_indent(&format_with(|f| {
                            let magic_trailing_comma = expr
                                .trivia
                                .iter()
                                .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
                            let is_unbroken =
                                expr.location.row() == expr.end_location.unwrap().row();
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
                        }))]
                    )?;
                } else {
                    let magic_trailing_comma = expr
                        .trivia
                        .iter()
                        .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
                    let is_unbroken = expr.location.row() == expr.end_location.unwrap().row();
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
                if matches!(expr.parentheses, Parenthesize::IfExpanded) {
                    write!(f, [if_group_breaks(&text(")"))])?;
                }
                Ok(())
            }))]
        )?;
    }
    Ok(())
}

fn format_slice(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    lower: &SliceIndex,
    upper: &SliceIndex,
    step: Option<&SliceIndex>,
) -> FormatResult<()> {
    // // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#slices
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

            // Apply any dangling comments.
            for trivia in &lower.trivia {
                if matches!(trivia.relationship, Relationship::Dangling) {
                    if let TriviaKind::OwnLineComment(range) = trivia.kind {
                        write!(f, [expand_parent()])?;
                        write!(f, [hard_line_break()])?;
                        write!(f, [literal(range)])?;
                        write!(f, [hard_line_break()])?;
                    }
                }
            }

            if matches!(lower.node, SliceIndexKind::Index { .. }) {
                if !is_simple {
                    write!(f, [space()])?;
                }
            }
            write!(f, [text(":")])?;

            // Format any end-of-line comments.
            let mut first = true;
            for range in lower.trivia.iter().filter_map(|trivia| {
                if matches!(trivia.relationship, Relationship::Trailing) {
                    if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                        Some(range)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }) {
                if std::mem::take(&mut first) {
                    write!(f, [line_suffix(&text("  "))])?;
                }
                write!(f, [line_suffix(&literal(range))])?;
            }

            if let SliceIndexKind::Index { value } = &upper.node {
                if !is_simple {
                    write!(f, [space()])?;
                }
                write!(f, [if_group_breaks(&soft_line_break())])?;
                write!(f, [value.format()])?;
            }

            // Apply any dangling comments.
            for trivia in &upper.trivia {
                if matches!(trivia.relationship, Relationship::Dangling) {
                    if let TriviaKind::OwnLineComment(range) = trivia.kind {
                        write!(f, [expand_parent()])?;
                        write!(f, [hard_line_break()])?;
                        write!(f, [literal(range)])?;
                        write!(f, [hard_line_break()])?;
                    }
                }
            }

            // Format any end-of-line comments.
            let mut first = true;
            for range in upper.trivia.iter().filter_map(|trivia| {
                if matches!(trivia.relationship, Relationship::Trailing) {
                    if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                        Some(range)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }) {
                if std::mem::take(&mut first) {
                    write!(f, [line_suffix(&text("  "))])?;
                }
                write!(f, [line_suffix(&literal(range))])?;
            }

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

                // Apply any dangling comments.
                for trivia in &step.trivia {
                    if matches!(trivia.relationship, Relationship::Dangling) {
                        if let TriviaKind::OwnLineComment(range) = trivia.kind {
                            write!(f, [expand_parent()])?;
                            write!(f, [hard_line_break()])?;
                            write!(f, [literal(range)])?;
                            write!(f, [hard_line_break()])?;
                        }
                    }
                }

                // Format any end-of-line comments.
                let mut first = true;
                for range in step.trivia.iter().filter_map(|trivia| {
                    if matches!(trivia.relationship, Relationship::Trailing) {
                        if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                            Some(range)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }) {
                    if std::mem::take(&mut first) {
                        write!(f, [line_suffix(&text("  "))])?;
                    }
                    write!(f, [line_suffix(&literal(range))])?;
                }
            }
            Ok(())
        }))]
    )?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_list(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    elts: &[Expr],
) -> FormatResult<()> {
    write!(f, [text("[")])?;
    if !elts.is_empty() {
        let magic_trailing_comma = expr
            .trivia
            .iter()
            .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
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
    Ok(())
}

fn format_set(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    elts: &[Expr],
) -> FormatResult<()> {
    if elts.is_empty() {
        write!(f, [text("set()")])?;
        Ok(())
    } else {
        write!(f, [text("{")])?;
        if !elts.is_empty() {
            let magic_trailing_comma = expr
                .trivia
                .iter()
                .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
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
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> FormatResult<()> {
    write!(f, [func.format()])?;
    if args.is_empty() && keywords.is_empty() {
        write!(f, [text("(")])?;
        write!(f, [text(")")])?;

        // Format any end-of-line comments.
        let mut first = true;
        for range in expr.trivia.iter().filter_map(|trivia| {
            if matches!(trivia.relationship, Relationship::Trailing) {
                if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                    Some(range)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            if std::mem::take(&mut first) {
                write!(f, [line_suffix(&text("  "))])?;
            }
            write!(f, [line_suffix(&literal(range))])?;
        }
    } else {
        write!(f, [text("(")])?;

        // Format any end-of-line comments.
        let mut first = true;
        for range in expr.trivia.iter().filter_map(|trivia| {
            if matches!(trivia.relationship, Relationship::Trailing) {
                if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                    Some(range)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            if std::mem::take(&mut first) {
                write!(f, [line_suffix(&text("  "))])?;
            }
            write!(f, [line_suffix(&literal(range))])?;
        }

        let magic_trailing_comma = expr
            .trivia
            .iter()
            .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
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
                            if let Some(arg) = &keyword.node.arg {
                                write!(f, [dynamic_text(arg, TextSize::default())])?;
                                write!(f, [text("=")])?;
                                write!(f, [keyword.node.value.format()])?;
                            } else {
                                write!(f, [text("**")])?;
                                write!(f, [keyword.node.value.format()])?;
                            }
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

                // Apply any dangling trailing comments.
                for trivia in &expr.trivia {
                    if matches!(trivia.relationship, Relationship::Dangling) {
                        if let TriviaKind::OwnLineComment(range) = trivia.kind {
                            write!(f, [expand_parent()])?;
                            write!(f, [hard_line_break()])?;
                            write!(f, [literal(range)])?;
                        }
                    }
                }

                Ok(())
            }))])]
        )?;
        write!(f, [text(")")])?;
    }
    Ok(())
}

fn format_list_comp(
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) -> FormatResult<()> {
    write!(f, [group(&format_args![left.format()])])?;
    for (i, op) in ops.iter().enumerate() {
        write!(f, [soft_line_break_or_space()])?;
        write!(f, [op.format()])?;
        write!(f, [space()])?;
        write!(f, [group(&format_args![comparators[i].format()])])?;
    }

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_joined_str(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    _values: &[Expr],
) -> FormatResult<()> {
    write!(f, [literal(Range::from_located(expr))])?;
    Ok(())
}

fn format_constant(
    f: &mut Formatter<ASTFormatContext<'_>>,
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
        Constant::Int(_) => write!(f, [int_literal(Range::from_located(expr))])?,
        Constant::Float(_) => write!(f, [float_literal(Range::from_located(expr))])?,
        Constant::Str(_) => write!(f, [string_literal(expr)])?,
        Constant::Bytes(_) => write!(f, [string_literal(expr)])?,
        Constant::Complex { .. } => write!(f, [complex_literal(Range::from_located(expr))])?,
        Constant::Tuple(_) => unreachable!("Constant::Tuple should be handled by format_tuple"),
    }
    Ok(())
}

fn format_dict(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    keys: &[Option<Expr>],
    values: &[Expr],
) -> FormatResult<()> {
    write!(f, [text("{")])?;
    if !keys.is_empty() {
        let magic_trailing_comma = expr
            .trivia
            .iter()
            .any(|c| matches!(c.kind, TriviaKind::MagicTrailingComma));
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
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    value: &Expr,
    attr: &str,
) -> FormatResult<()> {
    write!(f, [value.format()])?;
    write!(f, [text(".")])?;
    write!(f, [dynamic_text(attr, TextSize::default())])?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_bool_op(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    op: &Boolop,
    values: &[Expr],
) -> FormatResult<()> {
    let mut first = true;
    for value in values {
        if std::mem::take(&mut first) {
            write!(f, [group(&format_args![value.format()])])?;
        } else {
            write!(f, [soft_line_break_or_space()])?;
            write!(f, [op.format()])?;
            write!(f, [space()])?;
            write!(f, [group(&format_args![value.format()])])?;
        }
    }

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_bin_op(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    left: &Expr,
    op: &Operator,
    right: &Expr,
) -> FormatResult<()> {
    // https://black.readthedocs.io/en/stable/the_black_code_style/current_style.html#line-breaks-binary-operators
    let is_simple = matches!(op, Operator::Pow) && is_simple_power(left) && is_simple_power(right);

    write!(f, [left.format()])?;
    if !is_simple {
        write!(f, [soft_line_break_or_space()])?;
    }
    write!(f, [op.format()])?;
    if !is_simple {
        write!(f, [space()])?;
    }
    write!(f, [group(&format_args![right.format()])])?;

    // Format any end-of-line comments.
    let mut first = true;
    for range in expr.trivia.iter().filter_map(|trivia| {
        if matches!(trivia.relationship, Relationship::Trailing) {
            if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                Some(range)
            } else {
                None
            }
        } else {
            None
        }
    }) {
        if std::mem::take(&mut first) {
            write!(f, [line_suffix(&text("  "))])?;
        }
        write!(f, [line_suffix(&literal(range))])?;
    }

    Ok(())
}

fn format_unary_op(
    f: &mut Formatter<ASTFormatContext<'_>>,
    expr: &Expr,
    op: &Unaryop,
    operand: &Expr,
) -> FormatResult<()> {
    write!(f, [op.format()])?;
    // TODO(charlie): Do this in the normalization pass.
    if !matches!(op, Unaryop::Not)
        && matches!(
            operand.node,
            ExprKind::BoolOp { .. } | ExprKind::Compare { .. } | ExprKind::BinOp { .. }
        )
    {
        let parenthesized = matches!(operand.parentheses, Parenthesize::Always);
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
    Ok(())
}

fn format_lambda(
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    Ok(())
}

fn format_if_exp(
    f: &mut Formatter<ASTFormatContext<'_>>,
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
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        if matches!(self.item.parentheses, Parenthesize::Always) {
            write!(f, [text("(")])?;
        }

        // Any leading comments come on the line before.
        for trivia in &self.item.trivia {
            if matches!(trivia.relationship, Relationship::Leading) {
                if let TriviaKind::OwnLineComment(range) = trivia.kind {
                    write!(f, [expand_parent()])?;
                    write!(f, [literal(range)])?;
                    write!(f, [hard_line_break()])?;
                }
            }
        }

        match &self.item.node {
            ExprKind::BoolOp { op, values } => format_bool_op(f, self.item, op, values),
            // ExprKind::NamedExpr { .. } => {}
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
            // ExprKind::FormattedValue { .. } => {}
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
            _ => {
                unimplemented!("Implement ExprKind: {:?}", self.item.node)
            }
        }?;

        // Any trailing comments come on the lines after.
        for trivia in &self.item.trivia {
            if matches!(trivia.relationship, Relationship::Trailing) {
                if let TriviaKind::OwnLineComment(range) = trivia.kind {
                    write!(f, [expand_parent()])?;
                    write!(f, [literal(range)])?;
                    write!(f, [hard_line_break()])?;
                }
            }
        }

        if matches!(self.item.parentheses, Parenthesize::Always) {
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
