use crate::{AsFormat, FormatNodeRule, PyFormatter};
use ruff_formatter::prelude::{space, text};
use ruff_formatter::{write, Buffer, Format, FormatResult};
use rustpython_parser::ast::Alias;

#[derive(Default)]
pub struct FormatAlias;

impl FormatNodeRule<Alias> for FormatAlias {
    fn fmt_fields(&self, item: &Alias, f: &mut PyFormatter) -> FormatResult<()> {
        let Alias {
            range: _,
            name,
            asname,
        } = item;
        name.format().fmt(f)?;
        if let Some(asname) = asname {
            write!(f, [space(), text("as"), space(), asname.format()])?;
        }
        Ok(())
    }
}
