use ruff_formatter::write;
use ruff_python_ast::Alias;

use crate::other::identifier::DotDelimitedIdentifier;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatAlias;

impl FormatNodeRule<Alias> for FormatAlias {
    fn fmt_fields(&self, item: &Alias, f: &mut PyFormatter) -> FormatResult<()> {
        let Alias {
            range: _,
            name,
            asname,
        } = item;
        DotDelimitedIdentifier::new(name).fmt(f)?;
        if let Some(asname) = asname {
            write!(f, [space(), token("as"), space(), asname.format()])?;
        }
        Ok(())
    }
}
