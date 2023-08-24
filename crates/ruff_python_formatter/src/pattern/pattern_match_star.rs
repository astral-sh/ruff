use ruff_formatter::{prelude::text, write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchStar;

use crate::comments::{dangling_comments, SourceComment};
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar> for FormatPatternMatchStar {
    fn fmt_fields(&self, item: &PatternMatchStar, f: &mut PyFormatter) -> FormatResult<()> {
        let PatternMatchStar { name, .. } = item;

        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);

        write!(f, [text("*"), dangling_comments(dangling)])?;

        match name {
            Some(name) => write!(f, [name.format()]),
            None => write!(f, [text("_")]),
        }
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
