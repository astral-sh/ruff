use ruff_formatter::{prelude::text, write, Buffer, FormatResult};
use ruff_python_ast::PatternMatchStar;

use crate::comments::trailing_comments;
use crate::AsFormat;
use crate::{FormatNodeRule, PyFormatter};

#[derive(Default)]
pub struct FormatPatternMatchStar;

impl FormatNodeRule<PatternMatchStar> for FormatPatternMatchStar {
    fn fmt_fields(&self, item: &PatternMatchStar, f: &mut PyFormatter) -> FormatResult<()> {
        let comments = f.context().comments().clone();
        let dangling = comments.dangling(item);
        let trailing = trailing_comments(dangling);
        let PatternMatchStar { name, .. } = item;
        match name {
            Some(name) => write!(f, [text("*"), name.format(), trailing]),
            None => write!(f, [text("*_"), trailing]),
        }
    }
}
