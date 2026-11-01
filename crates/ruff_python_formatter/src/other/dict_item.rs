use ruff_formatter::{format_args, write};
use ruff_python_ast::DictItem;

use crate::comments::{dangling_comments, leading_comments};
use crate::prelude::*;

#[derive(Default)]
pub struct FormatDictItem;

impl FormatNodeRule<ruff_python_ast::kind::DictItem> for FormatDictItem {
    fn fmt_fields(&self, item: DictItem<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let value = item.value();
        let comments = f.context().comments().clone();
        if let Some(key) = item.key() {
            let dangling = comments.dangling(item);
            group(&format_with(|f| {
                key.format().fmt(f)?;
                token(":").fmt(f)?;
                if dangling.is_empty() {
                    space().fmt(f)?;
                } else {
                    dangling_comments(dangling).fmt(f)?;
                }
                value.format().fmt(f)
            }))
            .fmt(f)
        } else {
            // A comment on the unpacked value must precede the stars to keep them with the value.
            write!(
                f,
                [
                    leading_comments(comments.leading(value)),
                    group(&format_args![token("**"), value.format()])
                ]
            )
        }
    }
}
