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
        if let Some(arg) = arg {
            write!(f, [arg.format(), text("="), value.format()])
        } else {
            // Comments after the stars are reassigned as trailing value comments
            write!(f, [text("**"), value.format()])
        }
    }
}
