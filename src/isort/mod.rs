use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use itertools::Either::{Left, Right};
use itertools::Itertools;
use ropey::RopeBuilder;
use rustc_hash::FxHashMap;
use rustpython_ast::{Stmt, StmtKind};

use crate::isort::categorize::{categorize, ImportType};
use crate::isort::comments::Comment;
use crate::isort::helpers::trailing_comma;
use crate::isort::sorting::{cmp_import_froms, cmp_members, cmp_modules};
use crate::isort::track::{Block, Trailer};
use crate::isort::types::{
    AliasData, CommentSet, ImportBlock, ImportFromData, Importable, OrderedImportBlock,
    TrailingComma,
};
use crate::source_code_style::SourceCodeStyleDetector;
use crate::SourceCodeLocator;

mod categorize;
mod comments;
pub mod format;
mod helpers;
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
        trailing_comma: TrailingComma,
    },
}

fn annotate_imports<'a>(
    imports: &'a [&'a Stmt],
    comments: Vec<Comment<'a>>,
    locator: &SourceCodeLocator,
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
                    });
                }

                annotated.push(AnnotatedImport::ImportFrom {
                    module: module.as_ref(),
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

fn normalize_imports(imports: Vec<AnnotatedImport>, combine_as_imports: bool) -> ImportBlock {
    let mut block = ImportBlock::default();
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
                trailing_comma,
            } => {
                let single_import = names.len() == 1;

                // If we're dealing with a multi-import block (i.e., a non-star, non-aliased
                // import), associate the comments with the first alias (best
                // effort).
                if let Some(alias) = names.first() {
                    let entry = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if alias.asname.is_none() || combine_as_imports {
                        &mut block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                            .0
                    } else {
                        block
                            .import_from_as
                            .entry((
                                ImportFromData { module, level },
                                AliasData {
                                    name: alias.name,
                                    asname: alias.asname,
                                },
                            ))
                            .or_default()
                    };

                    for comment in atop {
                        entry.atop.push(comment.value);
                    }

                    // Associate inline comments with first alias if multiple names have been
                    // imported, i.e., the comment applies to all names; otherwise, associate
                    // with the alias.
                    if single_import
                        && (alias.name != "*" && (alias.asname.is_none() || combine_as_imports))
                    {
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
                        for comment in inline {
                            entry.inline.push(comment.value);
                        }
                    } else {
                        for comment in inline {
                            entry.inline.push(comment.value);
                        }
                    }
                }

                // Create an entry for every alias.
                for alias in names {
                    let entry = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if alias.asname.is_none() || combine_as_imports {
                        block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                            .1
                            .entry(AliasData {
                                name: alias.name,
                                asname: alias.asname,
                            })
                            .or_default()
                    } else {
                        block
                            .import_from_as
                            .entry((
                                ImportFromData { module, level },
                                AliasData {
                                    name: alias.name,
                                    asname: alias.asname,
                                },
                            ))
                            .or_default()
                    };

                    for comment in alias.atop {
                        entry.atop.push(comment.value);
                    }
                    for comment in alias.inline {
                        entry.inline.push(comment.value);
                    }
                }

                // Propagate trailing commas.
                if matches!(trailing_comma, TrailingComma::Present) {
                    if let Some(entry) =
                        block.import_from.get_mut(&ImportFromData { module, level })
                    {
                        entry.2 = trailing_comma;
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
    package: Option<&Path>,
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> BTreeMap<ImportType, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<ImportType, ImportBlock> = BTreeMap::default();
    // Categorize `StmtKind::Import`.
    for (alias, comments) in block.import {
        let import_type = categorize(
            &alias.module_base(),
            None,
            src,
            package,
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
            package,
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
            package,
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
    // Categorize `StmtKind::ImportFrom` (with star).
    for (import_from, comments) in block.import_from_star {
        let classification = categorize(
            &import_from.module_base(),
            import_from.level,
            src,
            package,
            known_first_party,
            known_third_party,
            extra_standard_library,
        );
        block_by_type
            .entry(classification)
            .or_default()
            .import_from_star
            .insert(import_from, comments);
    }
    block_by_type
}

fn sort_imports(block: ImportBlock) -> OrderedImportBlock {
    let mut ordered = OrderedImportBlock::default();

    // Sort `StmtKind::Import`.
    ordered.import.extend(
        block
            .import
            .into_iter()
            .sorted_by(|(alias1, _), (alias2, _)| cmp_modules(alias1, alias2)),
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
                                    inline: vec![],
                                },
                                FxHashMap::from_iter([(
                                    alias,
                                    CommentSet {
                                        atop: vec![],
                                        inline: comments.inline,
                                    },
                                )]),
                                TrailingComma::Absent,
                            ),
                        )
                    }),
            )
            .chain(
                // Include all star imports.
                block
                    .import_from_star
                    .into_iter()
                    .map(|(import_from, comments)| {
                        (
                            import_from,
                            (
                                CommentSet {
                                    atop: comments.atop,
                                    inline: vec![],
                                },
                                FxHashMap::from_iter([(
                                    AliasData {
                                        name: "*",
                                        asname: None,
                                    },
                                    CommentSet {
                                        atop: vec![],
                                        inline: comments.inline,
                                    },
                                )]),
                                TrailingComma::Absent,
                            ),
                        )
                    }),
            )
            .map(|(import_from, (comments, aliases, locations))| {
                // Within each `StmtKind::ImportFrom`, sort the members.
                (
                    import_from,
                    comments,
                    locations,
                    aliases
                        .into_iter()
                        .sorted_by(|(alias1, _), (alias2, _)| cmp_members(alias1, alias2))
                        .collect::<Vec<(AliasData, CommentSet)>>(),
                )
            })
            .sorted_by(
                |(import_from1, _, _, aliases1), (import_from2, _, _, aliases2)| {
                    cmp_import_froms(import_from1, import_from2).then_with(|| {
                        match (aliases1.first(), aliases2.first()) {
                            (None, None) => Ordering::Equal,
                            (None, Some(_)) => Ordering::Less,
                            (Some(_), None) => Ordering::Greater,
                            (Some((alias1, _)), Some((alias2, _))) => cmp_members(alias1, alias2),
                        }
                    })
                },
            ),
    );
    ordered
}

fn force_single_line_imports<'a>(
    block: OrderedImportBlock<'a>,
    single_line_exclusions: &BTreeSet<String>,
) -> OrderedImportBlock<'a> {
    OrderedImportBlock {
        import: block.import,
        import_from: block
            .import_from
            .into_iter()
            .flat_map(|(from_data, comment_set, trailing_comma, alias_data)| {
                if from_data
                    .module
                    .map_or(false, |module| single_line_exclusions.contains(module))
                {
                    Left(std::iter::once((
                        from_data,
                        comment_set,
                        trailing_comma,
                        alias_data,
                    )))
                } else {
                    Right(
                        alias_data
                            .into_iter()
                            .enumerate()
                            .map(move |(index, alias_data)| {
                                (
                                    from_data.clone(),
                                    if index == 0 {
                                        comment_set.clone()
                                    } else {
                                        CommentSet {
                                            atop: vec![],
                                            inline: vec![],
                                        }
                                    },
                                    TrailingComma::Absent,
                                    vec![alias_data],
                                )
                            }),
                    )
                }
            })
            .collect(),
    }
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn format_imports(
    block: &Block,
    comments: Vec<Comment>,
    locator: &SourceCodeLocator,
    line_length: usize,
    stylist: &SourceCodeStyleDetector,
    src: &[PathBuf],
    package: Option<&Path>,
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
    combine_as_imports: bool,
    force_wrap_aliases: bool,
    split_on_trailing_comma: bool,
    force_single_line: bool,
    single_line_exclusions: &BTreeSet<String>,
) -> String {
    let trailer = &block.trailer;
    let block = annotate_imports(&block.imports, comments, locator, split_on_trailing_comma);

    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(block, combine_as_imports);

    // Categorize by type (e.g., first-party vs. third-party).
    let block_by_type = categorize_imports(
        block,
        src,
        package,
        known_first_party,
        known_third_party,
        extra_standard_library,
    );

    let mut output = RopeBuilder::new();

    // Generate replacement source code.
    let mut is_first_block = true;
    for import_block in block_by_type.into_values() {
        let mut import_block = sort_imports(import_block);
        if force_single_line {
            import_block = force_single_line_imports(import_block, single_line_exclusions);
        }

        // Add a blank line between every section.
        if is_first_block {
            is_first_block = false;
        } else {
            output.append(stylist.line_ending());
        }

        let mut is_first_statement = true;

        // Format `StmtKind::Import` statements.
        for (alias, comments) in &import_block.import {
            output.append(&format::format_import(
                alias,
                comments,
                is_first_statement,
                stylist,
            ));
            is_first_statement = false;
        }

        // Format `StmtKind::ImportFrom` statements.
        for (import_from, comments, trailing_comma, aliases) in &import_block.import_from {
            output.append(&format::format_import_from(
                import_from,
                comments,
                aliases,
                line_length,
                stylist,
                force_wrap_aliases,
                is_first_statement,
                split_on_trailing_comma && matches!(trailing_comma, TrailingComma::Present),
            ));
            is_first_statement = false;
        }
    }
    match trailer {
        None => {}
        Some(Trailer::Sibling) => {
            output.append(stylist.line_ending());
        }
        Some(Trailer::FunctionDef | Trailer::ClassDef) => {
            output.append(stylist.line_ending());
            output.append(stylist.line_ending());
        }
    }
    output.finish().to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{isort, Settings};

    #[test_case(Path::new("add_newline_before_comments.py"))]
    #[test_case(Path::new("combine_as_imports.py"))]
    #[test_case(Path::new("combine_import_froms.py"))]
    #[test_case(Path::new("comments.py"))]
    #[test_case(Path::new("deduplicate_imports.py"))]
    #[test_case(Path::new("fit_line_length.py"))]
    #[test_case(Path::new("fit_line_length_comment.py"))]
    #[test_case(Path::new("force_wrap_aliases.py"))]
    #[test_case(Path::new("import_from_after_import.py"))]
    #[test_case(Path::new("insert_empty_lines.py"))]
    #[test_case(Path::new("insert_empty_lines.pyi"))]
    #[test_case(Path::new("leading_prefix.py"))]
    #[test_case(Path::new("natural_order.py"))]
    #[test_case(Path::new("no_reorder_within_section.py"))]
    #[test_case(Path::new("no_wrap_star.py"))]
    #[test_case(Path::new("order_by_type.py"))]
    #[test_case(Path::new("order_relative_imports_by_level.py"))]
    #[test_case(Path::new("preserve_comment_order.py"))]
    #[test_case(Path::new("preserve_import_star.py"))]
    #[test_case(Path::new("preserve_indentation.py"))]
    #[test_case(Path::new("reorder_within_section.py"))]
    #[test_case(Path::new("separate_first_party_imports.py"))]
    #[test_case(Path::new("separate_future_imports.py"))]
    #[test_case(Path::new("separate_local_folder_imports.py"))]
    #[test_case(Path::new("separate_third_party_imports.py"))]
    #[test_case(Path::new("skip.py"))]
    #[test_case(Path::new("skip_file.py"))]
    #[test_case(Path::new("sort_similar_imports.py"))]
    #[test_case(Path::new("split.py"))]
    #[test_case(Path::new("trailing_suffix.py"))]
    #[test_case(Path::new("type_comments.py"))]
    #[test_case(Path::new("magic_trailing_comma.py"))]
    #[test_case(Path::new("line_ending_lf.py"))]
    #[test_case(Path::new("line_ending_crlf.py"))]
    #[test_case(Path::new("line_ending_cr.py"))]
    fn default(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(CheckCode::I001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("combine_as_imports.py"))]
    fn combine_as_imports(path: &Path) -> Result<()> {
        let snapshot = format!("combine_as_imports_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: isort::settings::Settings {
                    combine_as_imports: true,
                    ..isort::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(CheckCode::I001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("force_wrap_aliases.py"))]
    fn force_wrap_aliases(path: &Path) -> Result<()> {
        let snapshot = format!("force_wrap_aliases_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: isort::settings::Settings {
                    force_wrap_aliases: true,
                    combine_as_imports: true,
                    ..isort::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(CheckCode::I001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("magic_trailing_comma.py"))]
    fn no_split_on_trailing_comma(path: &Path) -> Result<()> {
        let snapshot = format!("split_on_trailing_comma_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: isort::settings::Settings {
                    split_on_trailing_comma: false,
                    ..isort::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(CheckCode::I001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("force_single_line.py"))]
    fn force_single_line(path: &Path) -> Result<()> {
        let snapshot = format!("force_single_line_{}", path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: isort::settings::Settings {
                    force_single_line: true,
                    single_line_exclusions: vec!["os".to_string(), "logging.handlers".to_string()]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    ..isort::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(CheckCode::I001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
