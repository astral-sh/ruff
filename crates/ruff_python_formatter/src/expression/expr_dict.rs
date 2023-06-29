use crate::comments::{dangling_node_comments, leading_comments, Comments};
use crate::expression::parentheses::{
    default_expression_needs_parentheses, NeedsParentheses, Parentheses, Parenthesize,
};
use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::{format_args, write};
use ruff_text_size::TextRange;
use rustpython_parser::ast::Ranged;
use rustpython_parser::ast::{Expr, ExprDict};

#[derive(Default)]
pub struct FormatExprDict;

struct KeyValuePair<'a> {
    key: &'a Option<Expr>,
    value: &'a Expr,
}

impl Ranged for KeyValuePair<'_> {
    fn range(&self) -> TextRange {
        if let Some(key) = self.key {
            TextRange::new(key.start(), self.value.end())
        } else {
            self.value.range()
        }
    }
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
            let comments = f.context().comments().clone();
            let leading_value_comments = comments.leading_comments(self.value);
            write!(
                f,
                [
                    // make sure the leading comments are hoisted past the `**`
                    leading_comments(leading_value_comments),
                    group(&format_args![text("**"), self.value.format()])
                ]
            )
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

        debug_assert_eq!(keys.len(), values.len());

        if values.is_empty() {
            return write!(
                f,
                [
                    &text("{"),
                    block_indent(&dangling_node_comments(item)),
                    &text("}"),
                ]
            );
        }

        let format_pairs = format_with(|f| {
            let mut joiner = f.join_comma_separated();

            for (key, value) in keys.iter().zip(values) {
                let key_value_pair = KeyValuePair { key, value };
                joiner.entry(&key_value_pair, &key_value_pair);
            }

            joiner.finish()
        });

        write!(
            f,
            [group(&format_args![
                text("{"),
                soft_block_indent(&format_pairs),
                text("}")
            ])]
        )
    }

    fn fmt_dangling_comments(&self, _node: &ExprDict, _f: &mut PyFormatter) -> FormatResult<()> {
        // Handled by `fmt_fields`
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
