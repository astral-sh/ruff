use ruff_python_ast::token::Tokens;
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_trivia::is_pragma_comment;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::preview::is_pragma_kept_on_import_statement_enabled;
use crate::settings::types::PreviewMode;

use super::comments::Comment;
use super::helpers::trailing_comma;
use super::types::{AliasData, TrailingComma};
use super::{AnnotatedAliasData, AnnotatedImport};

pub(crate) fn annotate_imports<'a>(
    imports: &'a [&'a Stmt],
    comments: Vec<Comment<'a>>,
    locator: &Locator<'a>,
    split_on_trailing_comma: bool,
    tokens: &Tokens,
    preview: PreviewMode,
) -> Vec<AnnotatedImport<'a>> {
    let mut comments_iter = comments.into_iter().peekable();

    imports
        .iter()
        .map(|import| {
            match import {
                Stmt::Import(ast::StmtImport {
                    names,
                    range,
                    is_lazy,
                    node_index: _,
                }) => {
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
                                name: locator.slice(&alias.name),
                                asname: alias.asname.as_ref().map(|asname| locator.slice(asname)),
                                is_lazy: *is_lazy,
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
                    is_lazy,
                    range: _,
                    node_index: _,
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
                    let mut aliases: Vec<_> = names
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
                                name: locator.slice(&alias.name),
                                asname: alias.asname.as_ref().map(|asname| locator.slice(asname)),
                                atop: alias_atop,
                                inline: alias_inline,
                                trailing: vec![],
                            }
                        })
                        .collect();

                    // Capture trailing comments on the _last_ alias, as in:
                    // ```python
                    // from foo import (
                    //     bar,
                    //     # noqa
                    // )
                    // ```
                    if let Some(last_alias) = aliases.last_mut() {
                        while let Some(comment) =
                            comments_iter.next_if(|comment| comment.start() < import.end())
                        {
                            last_alias.trailing.push(comment);
                        }
                    }

                    // Capture trailing comments, as in:
                    // ```python
                    // from foo import (
                    //     bar,
                    // )  # noqa
                    // ```
                    let mut trailing = vec![];
                    let import_line_end = locator.line_end(import.end());
                    while let Some(comment) =
                        comments_iter.next_if(|comment| comment.start() < import_line_end)
                    {
                        trailing.push(comment);
                    }

                    // A comment at the end of a line that holds several aliases trails the
                    // whole line, not the alias that happens to come first on it. Splitting
                    // the import one alias per line would carry a pragma away from the names
                    // it suppressed, so move it to the statement instead.
                    if is_pragma_kept_on_import_statement_enabled(preview) {
                        for (alias, annotated) in names.iter().zip(&mut aliases) {
                            if !shares_line_with_another_alias(alias, names, locator) {
                                continue;
                            }
                            let (pragmas, rest): (Vec<_>, Vec<_>) = annotated
                                .inline
                                .drain(..)
                                .partition(|comment| is_pragma_comment(&comment.value));
                            annotated.inline = rest;
                            for pragma in pragmas {
                                if inline
                                    .iter()
                                    .all(|existing: &Comment| existing.value != pragma.value)
                                {
                                    inline.push(pragma);
                                }
                            }
                        }
                    }

                    AnnotatedImport::ImportFrom {
                        module: module.as_ref().map(|module| locator.slice(module)),
                        names: aliases,
                        level: *level,
                        is_lazy: *is_lazy,
                        trailing_comma: if split_on_trailing_comma {
                            trailing_comma(import, tokens)
                        } else {
                            TrailingComma::default()
                        },
                        atop,
                        inline,
                        trailing,
                    }
                }
                _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
            }
        })
        .collect()
}

/// Whether another alias of the same import sits on `alias`'s line.
fn shares_line_with_another_alias(
    alias: &ast::Alias,
    names: &[ast::Alias],
    locator: &Locator,
) -> bool {
    let line = locator.line_range(alias.start());
    names
        .iter()
        .any(|other| other.range() != alias.range() && line.contains_range(other.range()))
}
