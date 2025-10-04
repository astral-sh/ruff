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
        write!(f, [DotDelimitedIdentifier::new(name)])?;

        let comments = f.context().comments().clone();
        if comments.has_trailing(name) {
            write!(
                f,
                [
                    trailing_comments(comments.trailing(name)),
                    hard_line_break()
                ]
            )?;
        } else if asname.is_some() {
            write!(f, [space()])?;
        }

        if let Some(asname) = asname {
            write!(f, [token("as"),])?;

            if comments.has_leading(asname) {
                write!(
                    f,
                    [
                        trailing_comments(comments.leading(asname)),
                        hard_line_break()
                    ]
                )?;
            } else {
                write!(f, [space()])?;
            }

            write!(f, [asname.format()])?;
        }

        if comments.has_dangling(item) {
            write!(
                f,
                [
                    trailing_comments(comments.dangling(item)),
                    hard_line_break()
                ]
            )?;
        }
        Ok(())
    }
}
