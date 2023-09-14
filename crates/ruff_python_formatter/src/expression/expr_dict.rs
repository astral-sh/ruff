use ruff_formatter::{format_args, write};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::{Expr, ExprDict};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{leading_comments, SourceComment};
use crate::expression::parentheses::{
    empty_parenthesized, parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

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
    fn fmt(&self, f: &mut PyFormatter) -> FormatResult<()> {
        if let Some(key) = self.key {
            write!(
                f,
                [group(&format_args![
                    key.format(),
                    token(":"),
                    space(),
                    self.value.format()
                ])]
            )
        } else {
            // TODO(charlie): Make these dangling comments on the `ExprDict`, and identify them
            // dynamically, so as to avoid the parent rendering its child's comments.
            let comments = f.context().comments().clone();
            let leading_value_comments = comments.leading(self.value);
            write!(
                f,
                [
                    // make sure the leading comments are hoisted past the `**`
                    leading_comments(leading_value_comments),
                    group(&format_args![token("**"), self.value.format()])
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

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        if values.is_empty() {
            return empty_parenthesized("{", dangling, "}").fmt(f);
        }

        let format_pairs = format_with(|f| {
            let mut joiner = f.join_comma_separated(item.end());

            for (key, value) in keys.iter().zip(values) {
                let key_value_pair = KeyValuePair { key, value };
                joiner.entry(&key_value_pair, &key_value_pair);
            }

            joiner.finish()
        });

        parenthesized("{", &format_pairs, "}")
            .with_dangling_comments(dangling)
            .fmt(f)
    }

    fn fmt_dangling_comments(
        &self,
        _dangling_comments: &[SourceComment],
        _f: &mut PyFormatter,
    ) -> FormatResult<()> {
        // Handled by `fmt_fields`
        Ok(())
    }
}

impl NeedsParentheses for ExprDict {
    fn needs_parentheses(
        &self,
        _parent: AnyNodeRef,
        _context: &PyFormatContext,
    ) -> OptionalParentheses {
        OptionalParentheses::Never
    }
}
