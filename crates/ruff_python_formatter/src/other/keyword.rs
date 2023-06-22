use crate::prelude::*;
use crate::FormatNodeRule;
use ruff_formatter::write;
use rustpython_parser::ast::Keyword;

#[derive(Default)]
pub struct FormatKeyword;

impl FormatNodeRule<Keyword> for FormatKeyword {
    fn fmt_fields(&self, item: &Keyword, f: &mut PyFormatter) -> FormatResult<()> {
        let Keyword {
            range: _,
            arg,
            value,
        } = item;
        if let Some(argument) = arg {
            write!(f, [argument.format(), text("=")])?;
        }

        value.format().fmt(f)
    }
}
