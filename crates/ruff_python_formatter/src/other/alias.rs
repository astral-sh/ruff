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

        // ```python
        // from foo import (
        //     bar  # comment
        //     as baz,
        // )
        // ```
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
            write!(f, [token("as")])?;

            // ```python
            // from foo import (
            //     bar as  # comment
            //     baz,
            // )
            // ```
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

        // Dangling comment between alias and comma on a following line
        // ```python
        // from foo import (
        //     bar  # comment
        //     ,
        // )
        // ```
        let dangling = comments.dangling(item);
        if !dangling.is_empty() {
            write!(f, [trailing_comments(comments.dangling(item))])?;

            // Black will move the comma and merge comments if there is no own-line comment between
            // the alias and the comma.
            //
            // Eg:
            // ```python
            // from foo import (
            //     bar  # one
            //     ,  # two
            // )
            // ```
            //
            // Will become:
            // ```python
            // from foo import (
            //     bar,  # one  # two)
            // ```
            //
            // Only force a hard line break if an own-line dangling comment is present.
            if dangling
                .iter()
                .any(|comment| comment.line_position().is_own_line())
            {
                write!(f, [hard_line_break()])?;
            }
        }

        Ok(())
    }
}
