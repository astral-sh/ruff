use ruff_formatter::write;
use ruff_python_ast::Keyword;

use crate::prelude::*;

#[derive(Default)]
pub struct FormatKeyword;

impl FormatNodeRule<Keyword> for FormatKeyword {
    fn fmt_fields(&self, item: &Keyword, f: &mut PyFormatter) -> FormatResult<()> {
        let Keyword {
            range: _,
            arg,
            value,
        } = item;
        // Comments after the `=` or `**` are reassigned as leading comments on the value.
        if let Some(arg) = arg {
            write!(f, [arg.format(), token("="), value.format()])
        } else {
            write!(f, [token("**"), value.format()])
        }
    }
}
