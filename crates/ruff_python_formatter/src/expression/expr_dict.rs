use ruff_formatter::{format_args, write};
use ruff_python_ast::{AnyNodeRef, DictItem, Expr, ExprDict};
use ruff_text_size::{Ranged, TextRange};

use crate::comments::{dangling_comments, leading_comments, SourceComment};
use crate::expression::parentheses::{
    empty_parenthesized, parenthesized, NeedsParentheses, OptionalParentheses,
};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatExprDict;

impl FormatNodeRule<ExprDict> for FormatExprDict {
    fn fmt_fields(&self, item: &ExprDict, f: &mut PyFormatter) -> FormatResult<()> {
        let ExprDict { range: _, items } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        let Some(first_dict_item) = items.first() else {
            return empty_parenthesized("{", dangling, "}").fmt(f);
        };

        // Dangling comments can either appear after the open bracket, or around the key-value
        // pairs:
        // ```python
        // {  # open_parenthesis_comments
        //     x:  # key_value_comments
        //     y
        // }
        // ```
        let (open_parenthesis_comments, key_value_comments) =
            dangling.split_at(dangling.partition_point(|comment| {
                comment.end() < KeyValuePair::new(first_dict_item).start()
            }));

        let format_pairs = format_with(|f| {
            let mut joiner = f.join_comma_separated(item.end());

            let mut key_value_comments = key_value_comments;
            for dict_item in items {
                let mut key_value_pair = KeyValuePair::new(dict_item);

                let partition = key_value_comments
                    .partition_point(|comment| comment.start() < key_value_pair.end());
                key_value_pair = key_value_pair.with_comments(&key_value_comments[..partition]);
                key_value_comments = &key_value_comments[partition..];

                joiner.entry(&key_value_pair, &key_value_pair);
            }

            joiner.finish()
        });

        parenthesized("{", &format_pairs, "}")
            .with_dangling_comments(open_parenthesis_comments)
            .fmt(f)
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

#[derive(Debug)]
struct KeyValuePair<'a> {
    key: &'a Option<Expr>,
    value: &'a Expr,
    comments: &'a [SourceComment],
}

impl<'a> KeyValuePair<'a> {
    fn new(item: &'a DictItem) -> Self {
        Self {
            key: &item.key,
            value: &item.value,
            comments: &[],
        }
    }

    fn with_comments(self, comments: &'a [SourceComment]) -> Self {
        Self { comments, ..self }
    }
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
                [group(&format_with(|f| {
                    key.format().fmt(f)?;
                    token(":").fmt(f)?;

                    if self.comments.is_empty() {
                        space().fmt(f)?;
                    } else {
                        dangling_comments(self.comments).fmt(f)?;
                    }

                    self.value.format().fmt(f)
                }))]
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
