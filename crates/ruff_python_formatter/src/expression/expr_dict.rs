use crate::comments::{dangling_node_comments, Comments};
use crate::context::PyFormatContext;
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::trivia::Token;
use crate::trivia::{first_non_trivia_token, TokenKind};
use crate::USE_MAGIC_TRAILING_COMMA;
use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::format_args;
use ruff_formatter::{write, Buffer, FormatResult};
use ruff_python_ast::prelude::Ranged;
use rustpython_parser::ast::{Expr, ExprDict};

#[derive(Default)]
pub struct FormatExprDict;

struct KeyValuePair<'a> {
    key: &'a Option<Expr>,
    value: &'a Expr,
}

impl Format<PyFormatContext<'_>> for KeyValuePair<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        if let Some(key) = self.key {
            write!(
                f,
                [group(&format_args![
                    key.format(),
                    text(":"),
                    space(),
                    self.value.format()
                ])]
            )
        } else {
            write!(f, [group(&format_args![text("**"), self.value.format()])])
        }
    }
}

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, item: &ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprDict {
            range: _,
            keys,
            values,
        } = item;

        let last = match &values[..] {
            [] => {
                return write!(
                    f,
                    [
                        &text("{"),
                        block_indent(&dangling_node_comments(item)),
                        &text("}"),
                    ]
                );
            }
            [.., last] => last,
        };
        let magic_trailing_comma = USE_MAGIC_TRAILING_COMMA
            && matches!(
                first_non_trivia_token(last.range().end(), f.context().contents()),
                Some(Token {
                    kind: TokenKind::Comma,
                    ..
                })
            );

        debug_assert_eq!(keys.len(), values.len());

        let joined = format_with(|f| {
            f.join_with(format_args!(text(","), soft_line_break_or_space()))
                .entries(
                    keys.iter()
                        .zip(values)
                        .map(|(key, value)| KeyValuePair { key, value }),
                )
                .finish()
        });

        let block = if magic_trailing_comma {
            block_indent
        } else {
            soft_block_indent
        };

        write!(
            f,
            [group(&format_args![
                text("{"),
                block(&format_args![joined, if_group_breaks(&text(",")),]),
                text("}")
            ])]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprDict, _f: &mut PyFormatter) -> FormatResult<()> {
        // TODO(konstin): Reactivate when string formatting works, currently a source of unstable
        // formatting, e.g.
        // ```python
        // coverage_ignore_c_items = {
        // #    'cfunction': [...]
        // }
        // ```
        Ok(())
    }
}

impl NeedsParentheses for ExprDict {
    fn needs_parentheses(
        &self,
        parenthesize: Parenthesize,
        source: &str,
        comments: &Comments,
    ) -> Parentheses {
        match default_expression_needs_parentheses(self.into(), parenthesize, source, comments) {
            Parentheses::Optional => Parentheses::Never,
            parentheses => parentheses,
        }
    }
}
