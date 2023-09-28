use ruff_python_ast::{self as ast, PySourceType, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_source_file::Locator;

use super::comments::Comment;
use super::helpers::trailing_comma;
use super::types::{AliasData, TrailingComma};
use super::{AnnotatedAliasData, AnnotatedImport};

pub(crate) fn annotate_imports<'a>(
    imports: &'a [&'a Stmt],
    comments: Vec<Comment<'a>>,
    locator: &Locator,
    split_on_trailing_comma: bool,
    source_type: PySourceType,
) -> Vec<AnnotatedImport<'a>> {
    let mut comments_iter = comments.into_iter().peekable();

    imports
        .iter()
        .map(|import| {
            match import {
                Stmt::Import(ast::StmtImport { names, range }) => {
                    // Find comments above.
                    let mut atop = vec![];
                    while let Some(comment) =
                        comments_iter.next_if(|comment| comment.start() < range.start())
                    {
                        atop.push(comment);
                    }

                    // Find comments inline.
                    let mut inline = vec![];
                    let import_line_end = locator.line_end(range.end());

                    while let Some(comment) =
                        comments_iter.next_if(|comment| comment.end() <= import_line_end)
                    {
                        inline.push(comment);
                    }

                    AnnotatedImport::Import {
                        names: names
                            .iter()
                            .map(|alias| AliasData {
                                name: &alias.name,
                                asname: alias.asname.as_deref(),
                            })
                            .collect(),
                        atop,
                        inline,
                    }
                }
                Stmt::ImportFrom(ast::StmtImportFrom {
                    module,
                    names,
                    level,
                    range: _,
                }) => {
                    // Find comments above.
                    let mut atop = vec![];
                    while let Some(comment) =
                        comments_iter.next_if(|comment| comment.start() < import.start())
                    {
                        atop.push(comment);
                    }

                    // Find comments inline.
                    // We associate inline comments with the import statement unless there's a
                    // single member, and it's a single-line import (like `from foo
                    // import bar  # noqa`).
                    let mut inline = vec![];
                    if names.len() > 1
                        || names.first().is_some_and(|alias| {
                            locator
                                .contains_line_break(TextRange::new(import.start(), alias.start()))
                        })
                    {
                        let import_start_line_end = locator.line_end(import.start());
                        while let Some(comment) =
                            comments_iter.next_if(|comment| comment.end() <= import_start_line_end)
                        {
                            inline.push(comment);
                        }
                    }

                    // Capture names.
                    let aliases = names
                        .iter()
                        .map(|alias| {
                            // Find comments above.
                            let mut alias_atop = vec![];
                            while let Some(comment) =
                                comments_iter.next_if(|comment| comment.start() < alias.start())
                            {
                                alias_atop.push(comment);
                            }

                            // Find comments inline.
                            let mut alias_inline = vec![];
                            let alias_line_end = locator.line_end(alias.end());
                            while let Some(comment) =
                                comments_iter.next_if(|comment| comment.end() <= alias_line_end)
                            {
                                alias_inline.push(comment);
                            }

                            AnnotatedAliasData {
                                name: &alias.name,
                                asname: alias.asname.as_deref(),
                                atop: alias_atop,
                                inline: alias_inline,
                            }
                        })
                        .collect();

                    AnnotatedImport::ImportFrom {
                        module: module.as_deref(),
                        names: aliases,
                        level: *level,
                        trailing_comma: if split_on_trailing_comma {
                            trailing_comma(import, locator, source_type)
                        } else {
                            TrailingComma::default()
                        },
                        atop,
                        inline,
                    }
                }
                _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
            }
        })
        .collect()
}
