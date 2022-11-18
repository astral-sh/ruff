use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fnv::FnvHashMap;
use itertools::Itertools;
use ropey::RopeBuilder;
use rustpython_ast::{Stmt, StmtKind};

use crate::isort::categorize::{categorize, ImportType};
use crate::isort::comments::Comment;
use crate::isort::sorting::{member_key, module_key};
use crate::isort::types::{
    AliasData, CommentSet, ImportBlock, ImportFromData, Importable, OrderedImportBlock,
};

mod categorize;
mod comments;
pub mod format;
pub mod plugins;
pub mod settings;
mod sorting;
pub mod track;
mod types;

#[derive(Debug)]
pub struct AnnotatedAliasData<'a> {
    pub name: &'a str,
    pub asname: Option<&'a String>,
    pub atop: Vec<Comment<'a>>,
    pub inline: Vec<Comment<'a>>,
}
#[derive(Debug)]
pub enum AnnotatedImport<'a> {
    Import {
        names: Vec<AliasData<'a>>,
        atop: Vec<Comment<'a>>,
        inline: Vec<Comment<'a>>,
    },
    ImportFrom {
        module: Option<&'a String>,
        names: Vec<AnnotatedAliasData<'a>>,
        level: Option<&'a usize>,
        atop: Vec<Comment<'a>>,
        inline: Vec<Comment<'a>>,
    },
}

fn annotate_imports<'a>(
    imports: &'a [&'a Stmt],
    comments: Vec<Comment<'a>>,
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
                            asname: alias.node.asname.as_ref(),
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
                let mut inline = vec![];
                while let Some(comment) =
                    comments_iter.next_if(|comment| comment.location.row() == import.location.row())
                {
                    inline.push(comment);
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
                        asname: alias.node.asname.as_ref(),
                        atop: alias_atop,
                        inline: alias_inline,
                    })
                }

                annotated.push(AnnotatedImport::ImportFrom {
                    module: module.as_ref(),
                    names: aliases,
                    level: level.as_ref(),
                    atop,
                    inline,
                });
            }
            _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
        }
    }
    annotated
}

fn normalize_imports(imports: Vec<AnnotatedImport>) -> ImportBlock {
    let mut block: ImportBlock = Default::default();
    for import in imports {
        match import {
            AnnotatedImport::Import {
                names,
                atop,
                inline,
            } => {
                // Associate the comments with the first alias (best effort).
                if let Some(name) = names.first() {
                    let entry = block
                        .import
                        .entry(AliasData {
                            name: name.name,
                            asname: name.asname,
                        })
                        .or_default();
                    for comment in atop {
                        entry.atop.push(comment.value);
                    }
                    for comment in inline {
                        entry.inline.push(comment.value);
                    }
                }

                // Create an entry for every alias.
                for name in &names {
                    block
                        .import
                        .entry(AliasData {
                            name: name.name,
                            asname: name.asname,
                        })
                        .or_default();
                }
            }
            AnnotatedImport::ImportFrom {
                module,
                names,
                level,
                atop,
                inline,
            } => {
                // Associate the comments with the first alias (best effort).
                if let Some(alias) = names.first() {
                    if alias.asname.is_none() {
                        let entry = &mut block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                            .0;
                        for comment in atop {
                            entry.atop.push(comment.value);
                        }
                        for comment in inline {
                            entry.inline.push(comment.value);
                        }
                    } else {
                        let entry = &mut block
                            .import_from_as
                            .entry((
                                ImportFromData { module, level },
                                AliasData {
                                    name: alias.name,
                                    asname: alias.asname,
                                },
                            ))
                            .or_default();
                        for comment in atop {
                            entry.atop.push(comment.value);
                        }
                        for comment in inline {
                            entry.inline.push(comment.value);
                        }
                    }
                }

                // Create an entry for every alias.
                for alias in names {
                    if alias.asname.is_none() {
                        let entry = block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                            .1
                            .entry(AliasData {
                                name: alias.name,
                                asname: alias.asname,
                            })
                            .or_default();
                        for comment in alias.atop {
                            entry.atop.push(comment.value);
                        }
                        for comment in alias.inline {
                            entry.inline.push(comment.value);
                        }
                    } else {
                        let entry = block
                            .import_from_as
                            .entry((
                                ImportFromData { module, level },
                                AliasData {
                                    name: alias.name,
                                    asname: alias.asname,
                                },
                            ))
                            .or_default();
                        entry
                            .atop
                            .extend(alias.atop.into_iter().map(|comment| comment.value));
                        for comment in alias.inline {
                            entry.inline.push(comment.value);
                        }
                    }
                }
            }
        }
    }
    block
}

fn categorize_imports<'a>(
    block: ImportBlock<'a>,
    src: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> BTreeMap<ImportType, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<ImportType, ImportBlock> = Default::default();
    // Categorize `StmtKind::Import`.
    for (alias, comments) in block.import {
        let import_type = categorize(
            &alias.module_base(),
            None,
            src,
            known_first_party,
            known_third_party,
            extra_standard_library,
        );
        block_by_type
            .entry(import_type)
            .or_default()
            .import
            .insert(alias, comments);
    }
    // Categorize `StmtKind::ImportFrom` (without re-export).
    for (import_from, aliases) in block.import_from {
        let classification = categorize(
            &import_from.module_base(),
            import_from.level,
            src,
            known_first_party,
            known_third_party,
            extra_standard_library,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from
            .insert(import_from, aliases);
    }
    // Categorize `StmtKind::ImportFrom` (with re-export).
    for ((import_from, alias), comments) in block.import_from_as {
        let classification = categorize(
            &import_from.module_base(),
            import_from.level,
            src,
            known_first_party,
            known_third_party,
            extra_standard_library,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_as
            .insert((import_from, alias), comments);
    }
    block_by_type
}

fn sort_imports(block: ImportBlock) -> OrderedImportBlock {
    let mut ordered: OrderedImportBlock = Default::default();

    // Sort `StmtKind::Import`.
    ordered.import.extend(
        block
            .import
            .into_iter()
            .sorted_by_cached_key(|(alias, _)| module_key(alias.name, alias.asname)),
    );

    // Sort `StmtKind::ImportFrom`.
    ordered.import_from.extend(
        // Include all non-re-exports.
        block
            .import_from
            .into_iter()
            .chain(
                // Include all re-exports.
                block
                    .import_from_as
                    .into_iter()
                    .map(|((import_from, alias), comments)| {
                        (
                            import_from,
                            (
                                CommentSet {
                                    atop: comments.atop,
                                    inline: Default::default(),
                                },
                                FnvHashMap::from_iter([(
                                    alias,
                                    CommentSet {
                                        atop: Default::default(),
                                        inline: comments.inline,
                                    },
                                )]),
                            ),
                        )
                    }),
            )
            .map(|(import_from, (comments, aliases))| {
                // Within each `StmtKind::ImportFrom`, sort the members.
                (
                    import_from,
                    comments,
                    aliases
                        .into_iter()
                        .sorted_by_cached_key(|(alias, _)| member_key(alias.name, alias.asname))
                        .collect::<Vec<(AliasData, CommentSet)>>(),
                )
            })
            .sorted_by_cached_key(|(import_from, _, aliases)| {
                // Sort each `StmtKind::ImportFrom` by module key, breaking ties based on
                // members.
                (
                    import_from
                        .module
                        .as_ref()
                        .map(|module| module_key(module, None)),
                    aliases
                        .first()
                        .map(|(alias, _)| member_key(alias.name, alias.asname)),
                )
            }),
    );

    ordered
}

pub fn format_imports(
    block: &[&Stmt],
    comments: Vec<Comment>,
    line_length: &usize,
    src: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> String {
    let block = annotate_imports(block, comments);

    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(block);

    // Categorize by type (e.g., first-party vs. third-party).
    let block_by_type = categorize_imports(
        block,
        src,
        known_first_party,
        known_third_party,
        extra_standard_library,
    );

    let mut output = RopeBuilder::new();

    // Generate replacement source code.
    let mut is_first_block = true;
    for import_block in block_by_type.into_values() {
        let import_block = sort_imports(import_block);

        // Add a blank line between every section.
        if !is_first_block {
            output.append("\n");
        } else {
            is_first_block = false;
        }

        let mut is_first_statement = true;

        // Format `StmtKind::Import` statements.
        for (alias, comments) in import_block.import.iter() {
            output.append(&format::format_import(alias, comments, is_first_statement));
            is_first_statement = false;
        }

        // Format `StmtKind::ImportFrom` statements.
        for (import_from, comments, aliases) in import_block.import_from.iter() {
            output.append(&format::format_import_from(
                import_from,
                comments,
                aliases,
                line_length,
                is_first_statement,
            ));
            is_first_statement = false;
        }
    }
    output.finish().to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::autofix::fixer;
    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::Settings;

    #[test_case(Path::new("add_newline_before_comments.py"))]
    #[test_case(Path::new("combine_import_froms.py"))]
    #[test_case(Path::new("comments.py"))]
    #[test_case(Path::new("deduplicate_imports.py"))]
    #[test_case(Path::new("fit_line_length.py"))]
    #[test_case(Path::new("fit_line_length_comment.py"))]
    #[test_case(Path::new("import_from_after_import.py"))]
    #[test_case(Path::new("leading_prefix.py"))]
    #[test_case(Path::new("no_reorder_within_section.py"))]
    #[test_case(Path::new("order_by_type.py"))]
    #[test_case(Path::new("preserve_comment_order.py"))]
    #[test_case(Path::new("preserve_indentation.py"))]
    #[test_case(Path::new("reorder_within_section.py"))]
    #[test_case(Path::new("separate_first_party_imports.py"))]
    #[test_case(Path::new("separate_future_imports.py"))]
    #[test_case(Path::new("separate_local_folder_imports.py"))]
    #[test_case(Path::new("separate_third_party_imports.py"))]
    #[test_case(Path::new("skip.py"))]
    #[test_case(Path::new("sort_similar_imports.py"))]
    #[test_case(Path::new("trailing_suffix.py"))]
    #[test_case(Path::new("type_comments.py"))]
    fn isort(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(CheckCode::I001)
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
