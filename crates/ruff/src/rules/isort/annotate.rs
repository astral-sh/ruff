use rustpython_parser::ast::{Stmt, StmtKind};

use super::comments::Comment;
use super::helpers::trailing_comma;
use super::types::{AliasData, TrailingComma};
use super::{AnnotatedAliasData, AnnotatedImport};
use crate::source_code::Locator;

pub fn annotate_imports<'a>(
    imports: &'a [&'a Stmt],
    comments: Vec<Comment<'a>>,
    locator: &Locator,
    split_on_trailing_comma: bool,
) -> Vec<AnnotatedImport<'a>> {
    let mut annotated = vec![];
    let mut comments_iter = comments.into_iter().peekable();
    for import in imports {
        match &import.node {
            StmtKind::Import { names } => {
                // Find comments above.
                let mut atop = vec![];
                while let Some(comment) =
                    comments_iter.next_if(|comment| comment.location.row() < import.location.row())
                {
                    atop.push(comment);
                }

                // Find comments inline.
                let mut inline = vec![];
                while let Some(comment) = comments_iter.next_if(|comment| {
                    comment.end_location.row() == import.end_location.unwrap().row()
                }) {
                    inline.push(comment);
                }

                annotated.push(AnnotatedImport::Import {
                    names: names
                        .iter()
                        .map(|alias| AliasData {
                            name: &alias.node.name,
                            asname: alias.node.asname.as_deref(),
                        })
                        .collect(),
                    atop,
                    inline,
                });
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                // Find comments above.
                let mut atop = vec![];
                while let Some(comment) =
                    comments_iter.next_if(|comment| comment.location.row() < import.location.row())
                {
                    atop.push(comment);
                }

                // Find comments inline.
                // We associate inline comments with the import statement unless there's a
                // single member, and it's a single-line import (like `from foo
                // import bar  # noqa`).
                let mut inline = vec![];
                if names.len() > 1
                    || names
                        .first()
                        .map_or(false, |alias| alias.location.row() > import.location.row())
                {
                    while let Some(comment) = comments_iter
                        .next_if(|comment| comment.location.row() == import.location.row())
                    {
                        inline.push(comment);
                    }
                }

                // Capture names.
                let mut aliases = vec![];
                for alias in names {
                    // Find comments above.
                    let mut alias_atop = vec![];
                    while let Some(comment) = comments_iter
                        .next_if(|comment| comment.location.row() < alias.location.row())
                    {
                        alias_atop.push(comment);
                    }

                    // Find comments inline.
                    let mut alias_inline = vec![];
                    while let Some(comment) = comments_iter.next_if(|comment| {
                        comment.end_location.row() == alias.end_location.unwrap().row()
                    }) {
                        alias_inline.push(comment);
                    }

                    aliases.push(AnnotatedAliasData {
                        name: &alias.node.name,
                        asname: alias.node.asname.as_deref(),
                        atop: alias_atop,
                        inline: alias_inline,
                    });
                }

                annotated.push(AnnotatedImport::ImportFrom {
                    module: module.as_deref(),
                    names: aliases,
                    level: level.as_ref(),
                    trailing_comma: if split_on_trailing_comma {
                        trailing_comma(import, locator)
                    } else {
                        TrailingComma::default()
                    },
                    atop,
                    inline,
                });
            }
            _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
        }
    }
    annotated
}
