//! Rules from [isort](https://pypi.org/project/isort/).
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub use categorize::{categorize, ImportType};
use comments::Comment;
use helpers::trailing_comma;
use itertools::Either::{Left, Right};
use itertools::Itertools;
use rustc_hash::FxHashMap;
use rustpython_ast::{Stmt, StmtKind};
use settings::RelativeImportsOrder;
use sorting::{cmp_either_import, cmp_import_from, cmp_members, cmp_modules};
use track::{Block, Trailer};
use types::EitherImport::{Import, ImportFrom};
use types::{
    AliasData, CommentSet, EitherImport, ImportBlock, ImportFromData, Importable,
    OrderedImportBlock, TrailingComma,
};

use crate::source_code::{Locator, Stylist};

mod categorize;
mod comments;
mod format;
mod helpers;
pub(crate) mod rules;
pub mod settings;
mod sorting;
pub(crate) mod track;
mod types;

#[derive(Debug)]
pub struct AnnotatedAliasData<'a> {
    pub name: &'a str,
    pub asname: Option<&'a str>,
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
        module: Option<&'a str>,
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

                    for comment in inline {
                        entry.inline.push(comment.value);
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

fn order_imports<'a>(
    block: ImportBlock<'a>,
    order_by_type: bool,
    relative_imports_order: RelativeImportsOrder,
    classes: &'a BTreeSet<String>,
    constants: &'a BTreeSet<String>,
    variables: &'a BTreeSet<String>,
) -> OrderedImportBlock<'a> {
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
                        .sorted_by(|(alias1, _), (alias2, _)| {
                            cmp_members(
                                alias1,
                                alias2,
                                order_by_type,
                                classes,
                                constants,
                                variables,
                            )
                        })
                        .collect::<Vec<(AliasData, CommentSet)>>(),
                )
            })
            .sorted_by(
                |(import_from1, _, _, aliases1), (import_from2, _, _, aliases2)| {
                    cmp_import_from(import_from1, import_from2, relative_imports_order).then_with(
                        || match (aliases1.first(), aliases2.first()) {
                            (None, None) => Ordering::Equal,
                            (None, Some(_)) => Ordering::Less,
                            (Some(_), None) => Ordering::Greater,
                            (Some((alias1, _)), Some((alias2, _))) => cmp_members(
                                alias1,
                                alias2,
                                order_by_type,
                                classes,
                                constants,
                                variables,
                            ),
                        },
                    )
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
    locator: &Locator,
    line_length: usize,
    stylist: &Stylist,
    src: &[PathBuf],
    package: Option<&Path>,
    combine_as_imports: bool,
    extra_standard_library: &BTreeSet<String>,
    force_single_line: bool,
    force_sort_within_sections: bool,
    force_wrap_aliases: bool,
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    order_by_type: bool,
    relative_imports_order: RelativeImportsOrder,
    single_line_exclusions: &BTreeSet<String>,
    split_on_trailing_comma: bool,
    classes: &BTreeSet<String>,
    constants: &BTreeSet<String>,
    variables: &BTreeSet<String>,
    no_lines_before: &BTreeSet<ImportType>,
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

    let mut output = String::new();

    // Generate replacement source code.
    let mut is_first_block = true;
    for (import_type, import_block) in block_by_type {
        let mut imports = order_imports(
            import_block,
            order_by_type,
            relative_imports_order,
            classes,
            constants,
            variables,
        );

        if force_single_line {
            imports = force_single_line_imports(imports, single_line_exclusions);
        }

        let imports = {
            let mut imports = imports
                .import
                .into_iter()
                .map(Import)
                .chain(imports.import_from.into_iter().map(ImportFrom))
                .collect::<Vec<EitherImport>>();
            if force_sort_within_sections {
                imports.sort_by(|import1, import2| {
                    cmp_either_import(import1, import2, relative_imports_order)
                });
            };
            imports
        };

        // Add a blank line between every section.
        if is_first_block {
            is_first_block = false;
        } else if !no_lines_before.contains(&import_type) {
            output.push_str(stylist.line_ending());
        }

        let mut is_first_statement = true;
        for import in imports {
            match import {
                Import((alias, comments)) => {
                    output.push_str(&format::format_import(
                        &alias,
                        &comments,
                        is_first_statement,
                        stylist,
                    ));
                }
                ImportFrom((import_from, comments, trailing_comma, aliases)) => {
                    output.push_str(&format::format_import_from(
                        &import_from,
                        &comments,
                        &aliases,
                        line_length,
                        stylist,
                        force_wrap_aliases,
                        is_first_statement,
                        split_on_trailing_comma && matches!(trailing_comma, TrailingComma::Present),
                    ));
                }
            }
            is_first_statement = false;
        }
    }
    match trailer {
        None => {}
        Some(Trailer::Sibling) => {
            output.push_str(stylist.line_ending());
        }
        Some(Trailer::FunctionDef | Trailer::ClassDef) => {
            output.push_str(stylist.line_ending());
            output.push_str(stylist.line_ending());
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::categorize::ImportType;
    use super::settings::RelativeImportsOrder;
    use crate::assert_yaml_snapshot;
    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings::Settings;

    #[test_case(Path::new("add_newline_before_comments.py"))]
    #[test_case(Path::new("combine_as_imports.py"))]
    #[test_case(Path::new("combine_import_from.py"))]
    #[test_case(Path::new("comments.py"))]
    #[test_case(Path::new("deduplicate_imports.py"))]
    #[test_case(Path::new("fit_line_length.py"))]
    #[test_case(Path::new("fit_line_length_comment.py"))]
    #[test_case(Path::new("force_sort_within_sections.py"))]
    #[test_case(Path::new("force_wrap_aliases.py"))]
    #[test_case(Path::new("import_from_after_import.py"))]
    #[test_case(Path::new("inline_comments.py"))]
    #[test_case(Path::new("insert_empty_lines.py"))]
    #[test_case(Path::new("insert_empty_lines.pyi"))]
    #[test_case(Path::new("leading_prefix.py"))]
    #[test_case(Path::new("magic_trailing_comma.py"))]
    #[test_case(Path::new("natural_order.py"))]
    #[test_case(Path::new("no_reorder_within_section.py"))]
    #[test_case(Path::new("no_wrap_star.py"))]
    #[test_case(Path::new("order_by_type.py"))]
    #[test_case(Path::new("order_relative_imports_by_level.py"))]
    #[test_case(Path::new("preserve_comment_order.py"))]
    #[test_case(Path::new("preserve_import_star.py"))]
    #[test_case(Path::new("preserve_indentation.py"))]
    #[test_case(Path::new("reorder_within_section.py"))]
    #[test_case(Path::new("relative_imports_order.py"))]
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
    #[test_case(Path::new("order_by_type_with_custom_classes.py"))]
    #[test_case(Path::new("order_by_type_with_custom_constants.py"))]
    #[test_case(Path::new("order_by_type_with_custom_variables.py"))]
    #[test_case(Path::new("no_lines_before.py"))]
    fn default(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    // Test currently disabled as line endings are automatically converted to
    // platform-appropriate ones in CI/CD #[test_case(Path::new("
    // line_ending_crlf.py"))] #[test_case(Path::new("line_ending_lf.py"))]
    // fn source_code_style(path: &Path) -> Result<()> {
    //     let snapshot = format!("{}", path.to_string_lossy());
    //     let diagnostics = test_path(
    //         Path::new("./resources/test/fixtures/isort")
    //             .join(path)
    //             .as_path(),
    //         &Settings {
    //             src:
    // vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
    //             ..Settings::for_rule(Rule::UnsortedImports)
    //         },
    //     )?;
    //     insta::assert_yaml_snapshot!(snapshot, diagnostics);
    //     Ok(())
    // }

    #[test_case(Path::new("combine_as_imports.py"))]
    fn combine_as_imports(path: &Path) -> Result<()> {
        let snapshot = format!("combine_as_imports_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    combine_as_imports: true,
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_wrap_aliases.py"))]
    fn force_wrap_aliases(path: &Path) -> Result<()> {
        let snapshot = format!("force_wrap_aliases_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_wrap_aliases: true,
                    combine_as_imports: true,
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("magic_trailing_comma.py"))]
    fn no_split_on_trailing_comma(path: &Path) -> Result<()> {
        let snapshot = format!("split_on_trailing_comma_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    split_on_trailing_comma: false,
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_single_line.py"))]
    fn force_single_line(path: &Path) -> Result<()> {
        let snapshot = format!("force_single_line_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_single_line: true,
                    single_line_exclusions: vec!["os".to_string(), "logging.handlers".to_string()]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type.py"))]
    fn order_by_type(path: &Path) -> Result<()> {
        let snapshot = format!("order_by_type_false_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    order_by_type: false,
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type_with_custom_classes.py"))]
    fn order_by_type_with_custom_classes(path: &Path) -> Result<()> {
        let snapshot = format!(
            "order_by_type_with_custom_classes_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    order_by_type: true,
                    classes: BTreeSet::from([
                        "SVC".to_string(),
                        "SELU".to_string(),
                        "N_CLASS".to_string(),
                        "CLASS".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type_with_custom_constants.py"))]
    fn order_by_type_with_custom_constants(path: &Path) -> Result<()> {
        let snapshot = format!(
            "order_by_type_with_custom_constants_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    order_by_type: true,
                    constants: BTreeSet::from([
                        "Const".to_string(),
                        "constant".to_string(),
                        "First".to_string(),
                        "Last".to_string(),
                        "A_constant".to_string(),
                        "konst".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type_with_custom_variables.py"))]
    fn order_by_type_with_custom_variables(path: &Path) -> Result<()> {
        let snapshot = format!(
            "order_by_type_with_custom_variables_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    order_by_type: true,
                    variables: BTreeSet::from([
                        "VAR".to_string(),
                        "Variable".to_string(),
                        "MyVar".to_string(),
                        "var_ABC".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_sort_within_sections.py"))]
    fn force_sort_within_sections(path: &Path) -> Result<()> {
        let snapshot = format!("force_sort_within_sections_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_sort_within_sections: true,
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn required_import(path: &Path) -> Result<()> {
        let snapshot = format!("required_import_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort/required_imports")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from([
                        "from __future__ import annotations".to_string()
                    ]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn required_imports(path: &Path) -> Result<()> {
        let snapshot = format!("required_imports_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort/required_imports")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from([
                        "from __future__ import annotations".to_string(),
                        "from __future__ import generator_stop".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn combined_required_imports(path: &Path) -> Result<()> {
        let snapshot = format!("combined_required_imports_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort/required_imports")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from(["from __future__ import annotations, \
                                                       generator_stop"
                        .to_string()]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn straight_required_import(path: &Path) -> Result<()> {
        let snapshot = format!("straight_required_import_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort/required_imports")
                .join(path)
                .as_path(),
            &Settings {
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from(["import os".to_string()]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("relative_imports_order.py"))]
    fn closest_to_furthest(path: &Path) -> Result<()> {
        let snapshot = format!("closest_to_furthest_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    relative_imports_order: RelativeImportsOrder::ClosestToFurthest,
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("no_lines_before.py"))]
    fn no_lines_before(path: &Path) -> Result<()> {
        let snapshot = format!("no_lines_before.py_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("./resources/test/fixtures/isort")
                .join(path)
                .as_path(),
            &Settings {
                isort: super::settings::Settings {
                    no_lines_before: BTreeSet::from([
                        ImportType::Future,
                        ImportType::StandardLibrary,
                        ImportType::ThirdParty,
                        ImportType::FirstParty,
                        ImportType::LocalFolder,
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![Path::new("resources/test/fixtures/isort").to_path_buf()],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
