use ruff_formatter::write;
use ruff_python_ast::Alias;

use crate::comments::trailing_comments;
use crate::other::identifier::DotDelimitedIdentifier;
use crate::prelude::*;

#[derive(Default)]
pub struct FormatAlias;

impl FormatNodeRule<Alias> for FormatAlias {
    fn fmt_fields(&self, item: &Alias, f: &mut PyFormatter) -> FormatResult<()> {
        let Alias {
            range: _,
            node_index: _,
            name,
            asname,
        } = item;
        DotDelimitedIdentifier::new(name).fmt(f)?;
        if let Some(asname) = asname {
            let comments = f.context().comments().clone();
            let dangling = comments.dangling(item);
            write!(
                f,
                [
                    space(),
                    token("as"),
                    space(),
                    asname.format(),
                    trailing_comments(dangling),
                ]
            )?;
        }
        Ok(())
    }
}
