use std::usize;

use rustpython_parser::ast::{Arguments, Ranged};

use ruff_formatter::{format_args, write};
use ruff_python_ast::node::{AnyNodeRef, AstNode};

use crate::comments::{dangling_node_comments, leading_node_comments};
use crate::context::NodeLevel;
use crate::prelude::*;
use crate::trivia::{first_non_trivia_token, SimpleTokenizer, Token, TokenKind};
use crate::FormatNodeRule;

#[derive(Default)]
pub struct FormatArguments;

impl FormatNodeRule<Arguments> for FormatArguments {
    fn fmt_fields(&self, item: &Arguments, f: &mut PyFormatter) -> FormatResult<()> {
        let Arguments {
            range: _,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = item;

        let saved_level = f.context().node_level();
        f.context_mut().set_node_level(NodeLevel::Expression);

        let format_inner = format_with(|f: &mut PyFormatter| {
            let separator = format_with(|f| write!(f, [text(","), soft_line_break_or_space()]));
            let mut joiner = f.join_with(separator);
            let mut last_node: Option<AnyNodeRef> = None;

            for arg_with_default in posonlyargs {
                joiner.entry(&arg_with_default.into_format());

                last_node = Some(
                    arg_with_default
                        .default
                        .as_deref()
                        .map_or_else(|| (&arg_with_default.def).into(), AnyNodeRef::from),
                );
            }

            if !posonlyargs.is_empty() {
                joiner.entry(&text("/"));
            }

            for arg_with_default in args {
                joiner.entry(&arg_with_default.into_format());

                last_node = Some(
                    arg_with_default
                        .default
                        .as_deref()
                        .map_or_else(|| (&arg_with_default.def).into(), AnyNodeRef::from),
                );
            }

            // kw only args need either a `*args` ahead of them capturing all var args or a `*`
            // pseudo-argument capturing all fields. We can also have `*args` without any kwargs
            // afterwards.
            if let Some(vararg) = vararg {
                joiner.entry(&format_args![
                    leading_node_comments(vararg.as_ref()),
                    text("*"),
                    vararg.format()
                ]);
                last_node = Some(vararg.as_any_node_ref());
            } else if !kwonlyargs.is_empty() {
                joiner.entry(&text("*"));
            }

            for arg_with_default in kwonlyargs {
                joiner.entry(&arg_with_default.into_format());

                last_node = Some(
                    arg_with_default
                        .default
                        .as_deref()
                        .map_or_else(|| (&arg_with_default.def).into(), AnyNodeRef::from),
                );
            }

            if let Some(kwarg) = kwarg {
                joiner.entry(&format_args![
                    leading_node_comments(kwarg.as_ref()),
                    text("**"),
                    kwarg.format()
                ]);
                last_node = Some(kwarg.as_any_node_ref());
            }

            joiner.finish()?;

            write!(f, [if_group_breaks(&text(","))])?;

            // Expand the group if the source has a trailing *magic* comma.
            if let Some(last_node) = last_node {
                let ends_with_pos_only_argument_separator = !posonlyargs.is_empty()
                    && args.is_empty()
                    && vararg.is_none()
                    && kwonlyargs.is_empty()
                    && kwarg.is_none();

                let maybe_comma_token = if ends_with_pos_only_argument_separator {
                    // `def a(b, c, /): ... `
                    let mut tokens =
                        SimpleTokenizer::starts_at(last_node.end(), f.context().contents())
                            .skip_trivia();

                    let comma = tokens.next();
                    assert!(matches!(comma, Some(Token { kind: TokenKind::Comma, .. })), "The last positional only argument must be separated by a `,` from the positional only arguments separator `/` but found '{comma:?}'.");

                    let slash = tokens.next();
                    assert!(matches!(slash, Some(Token { kind: TokenKind::Slash, .. })), "The positional argument separator must be present for a function that has positional only arguments but found '{slash:?}'.");

                    tokens.next()
                } else {
                    first_non_trivia_token(last_node.end(), f.context().contents())
                };

                if maybe_comma_token.map_or(false, |token| token.kind() == TokenKind::Comma) {
                    write!(f, [hard_line_break()])?;
                }
            }

            Ok(())
        });

        let num_arguments = posonlyargs.len()
            + args.len()
            + usize::from(vararg.is_some())
            + kwonlyargs.len()
            + usize::from(kwarg.is_some());

        if num_arguments == 0 {
            // No arguments, format any dangling comments between `()`
            write!(
                f,
                [
                    text("("),
                    block_indent(&dangling_node_comments(item)),
                    text(")")
                ]
            )?;
        } else {
            write!(
                f,
                [group(&format_args!(
                    text("("),
                    soft_block_indent(&group(&format_inner)),
                    text(")")
                ))]
            )?;
        }

        f.context_mut().set_node_level(saved_level);

        Ok(())
    }

    fn fmt_dangling_comments(&self, _node: &Arguments, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled in `fmt_fields`
        Ok(())
    }
}
