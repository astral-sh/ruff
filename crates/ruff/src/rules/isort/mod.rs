//! Rules from [isort](https://pypi.org/project/isort/).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use itertools::Either::{Left, Right};

use annotate::annotate_imports;
use categorize::categorize_imports;
pub use categorize::{categorize, ImportSection, ImportType};
use comments::Comment;
use normalize::normalize_imports;
use order::order_imports;
use ruff_python_ast::source_code::{Locator, Stylist};
use settings::RelativeImportsOrder;
use sorting::cmp_either_import;
use track::{Block, Trailer};
use types::EitherImport::{Import, ImportFrom};
use types::{AliasData, CommentSet, EitherImport, OrderedImportBlock, TrailingComma};

use crate::rules::isort::categorize::KnownModules;
use crate::rules::isort::types::ImportBlock;
use crate::settings::types::PythonVersion;

mod annotate;
mod categorize;
mod comments;
mod format;
mod helpers;
mod normalize;
mod order;
pub(crate) mod rules;
pub mod settings;
mod sorting;
mod split;
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
        level: Option<usize>,
        atop: Vec<Comment<'a>>,
        inline: Vec<Comment<'a>>,
        trailing_comma: TrailingComma,
    },
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
                                            inline: comment_set.inline.clone(),
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
    force_single_line: bool,
    force_sort_within_sections: bool,
    force_wrap_aliases: bool,
    force_to_top: &BTreeSet<String>,
    known_modules: &KnownModules,
    order_by_type: bool,
    relative_imports_order: RelativeImportsOrder,
    single_line_exclusions: &BTreeSet<String>,
    split_on_trailing_comma: bool,
    classes: &BTreeSet<String>,
    constants: &BTreeSet<String>,
    variables: &BTreeSet<String>,
    no_lines_before: &BTreeSet<ImportSection>,
    lines_after_imports: isize,
    lines_between_types: usize,
    forced_separate: &[String],
    target_version: PythonVersion,
    section_order: &[ImportSection],
) -> String {
    let trailer = &block.trailer;
    let block = annotate_imports(&block.imports, comments, locator, split_on_trailing_comma);

    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(block, combine_as_imports, force_single_line);

    // Categorize imports.
    let mut output = String::new();

    for block in split::split_by_forced_separate(block, forced_separate) {
        let block_output = format_import_block(
            block,
            line_length,
            stylist,
            src,
            package,
            force_single_line,
            force_sort_within_sections,
            force_wrap_aliases,
            force_to_top,
            known_modules,
            order_by_type,
            relative_imports_order,
            single_line_exclusions,
            split_on_trailing_comma,
            classes,
            constants,
            variables,
            no_lines_before,
            lines_between_types,
            target_version,
            section_order,
        );

        if !block_output.is_empty() && !output.is_empty() {
            // If we are about to output something, and had already
            // output a block, separate them.
            output.push_str(&stylist.line_ending());
        }
        output.push_str(block_output.as_str());
    }

    match trailer {
        None => {}
        Some(Trailer::Sibling) => {
            if lines_after_imports >= 0 {
                for _ in 0..lines_after_imports {
                    output.push_str(&stylist.line_ending());
                }
            } else {
                output.push_str(&stylist.line_ending());
            }
        }
        Some(Trailer::FunctionDef | Trailer::ClassDef) => {
            if lines_after_imports >= 0 {
                for _ in 0..lines_after_imports {
                    output.push_str(&stylist.line_ending());
                }
            } else {
                output.push_str(&stylist.line_ending());
                output.push_str(&stylist.line_ending());
            }
        }
    }
    output
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn format_import_block(
    block: ImportBlock,
    line_length: usize,
    stylist: &Stylist,
    src: &[PathBuf],
    package: Option<&Path>,
    force_single_line: bool,
    force_sort_within_sections: bool,
    force_wrap_aliases: bool,
    force_to_top: &BTreeSet<String>,
    known_modules: &KnownModules,
    order_by_type: bool,
    relative_imports_order: RelativeImportsOrder,
    single_line_exclusions: &BTreeSet<String>,
    split_on_trailing_comma: bool,
    classes: &BTreeSet<String>,
    constants: &BTreeSet<String>,
    variables: &BTreeSet<String>,
    no_lines_before: &BTreeSet<ImportSection>,
    lines_between_types: usize,
    target_version: PythonVersion,
    section_order: &[ImportSection],
) -> String {
    // Categorize by type (e.g., first-party vs. third-party).
    let mut block_by_type = categorize_imports(block, src, package, known_modules, target_version);

    let mut output = String::new();

    // Generate replacement source code.
    let mut is_first_block = true;
    let mut pending_lines_before = false;
    for import_section in section_order {
        let import_block = block_by_type.remove(import_section);

        if !no_lines_before.contains(import_section) {
            pending_lines_before = true;
        }
        let Some(import_block) = import_block else {
            continue;
        };

        let mut imports = order_imports(
            import_block,
            order_by_type,
            relative_imports_order,
            classes,
            constants,
            variables,
            force_to_top,
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
                    cmp_either_import(import1, import2, relative_imports_order, force_to_top)
                });
            };
            imports
        };

        // Add a blank line between every section.
        if is_first_block {
            is_first_block = false;
            pending_lines_before = false;
        } else if pending_lines_before {
            output.push_str(&stylist.line_ending());
            pending_lines_before = false;
        }

        let mut lines_inserted = false;
        let mut has_direct_import = false;
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

                    has_direct_import = true;
                }

                ImportFrom((import_from, comments, trailing_comma, aliases)) => {
                    // Add a blank lines between direct and from imports
                    if lines_between_types > 0 && has_direct_import && !lines_inserted {
                        for _ in 0..lines_between_types {
                            output.push_str(&stylist.line_ending());
                        }

                        lines_inserted = true;
                    }

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
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::rules::isort::categorize::{ImportSection, KnownModules};
    use crate::settings::Settings;
    use crate::test::{test_path, test_resource_path};

    use super::categorize::ImportType;
    use super::settings::RelativeImportsOrder;

    #[test_case(Path::new("add_newline_before_comments.py"))]
    #[test_case(Path::new("as_imports_comments.py"))]
    #[test_case(Path::new("combine_as_imports.py"))]
    #[test_case(Path::new("combine_import_from.py"))]
    #[test_case(Path::new("comments.py"))]
    #[test_case(Path::new("deduplicate_imports.py"))]
    #[test_case(Path::new("fit_line_length.py"))]
    #[test_case(Path::new("fit_line_length_comment.py"))]
    #[test_case(Path::new("force_sort_within_sections.py"))]
    #[test_case(Path::new("force_to_top.py"))]
    #[test_case(Path::new("force_wrap_aliases.py"))]
    #[test_case(Path::new("import_from_after_import.py"))]
    #[test_case(Path::new("inline_comments.py"))]
    #[test_case(Path::new("insert_empty_lines.py"))]
    #[test_case(Path::new("insert_empty_lines.pyi"))]
    #[test_case(Path::new("isort_skip_file.py"))]
    #[test_case(Path::new("leading_prefix.py"))]
    #[test_case(Path::new("magic_trailing_comma.py"))]
    #[test_case(Path::new("natural_order.py"))]
    #[test_case(Path::new("no_lines_before.py"))]
    #[test_case(Path::new("no_reorder_within_section.py"))]
    #[test_case(Path::new("no_wrap_star.py"))]
    #[test_case(Path::new("order_by_type.py"))]
    #[test_case(Path::new("order_by_type_with_custom_classes.py"))]
    #[test_case(Path::new("order_by_type_with_custom_constants.py"))]
    #[test_case(Path::new("order_by_type_with_custom_variables.py"))]
    #[test_case(Path::new("order_relative_imports_by_level.py"))]
    #[test_case(Path::new("preserve_comment_order.py"))]
    #[test_case(Path::new("preserve_import_star.py"))]
    #[test_case(Path::new("preserve_indentation.py"))]
    #[test_case(Path::new("preserve_tabs.py"))]
    #[test_case(Path::new("preserve_tabs_2.py"))]
    #[test_case(Path::new("relative_imports_order.py"))]
    #[test_case(Path::new("reorder_within_section.py"))]
    #[test_case(Path::new("ruff_skip_file.py"))]
    #[test_case(Path::new("separate_first_party_imports.py"))]
    #[test_case(Path::new("separate_future_imports.py"))]
    #[test_case(Path::new("separate_local_folder_imports.py"))]
    #[test_case(Path::new("separate_third_party_imports.py"))]
    #[test_case(Path::new("skip.py"))]
    #[test_case(Path::new("sort_similar_imports.py"))]
    #[test_case(Path::new("split.py"))]
    #[test_case(Path::new("star_before_others.py"))]
    #[test_case(Path::new("trailing_suffix.py"))]
    #[test_case(Path::new("type_comments.py"))]
    fn default(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("separate_subpackage_first_and_third_party_imports.py"))]
    fn separate_modules(path: &Path) -> Result<()> {
        let snapshot = format!("1_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec!["foo.bar".to_string(), "baz".to_string()],
                        vec!["foo".to_string(), "__future__".to_string()],
                        vec![],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("separate_subpackage_first_and_third_party_imports.py"))]
    fn separate_modules_first_party(path: &Path) -> Result<()> {
        let snapshot = format!("2_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec!["foo".to_string()],
                        vec!["foo.bar".to_string()],
                        vec![],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    // Test currently disabled as line endings are automatically converted to
    // platform-appropriate ones in CI/CD #[test_case(Path::new("
    // line_ending_crlf.py"))] #[test_case(Path::new("line_ending_lf.py"))]
    // fn source_code_style(path: &Path) -> Result<()> {
    //     let snapshot = format!("{}", path.to_string_lossy());
    //     let diagnostics = test_path(
    //         Path::new("isort")
    //             .join(path)
    //             .as_path(),
    //         &Settings {
    //             src: vec![test_resource_path("fixtures/isort")],
    //             ..Settings::for_rule(Rule::UnsortedImports)
    //         },
    //     )?;
    //     crate::assert_messages!(snapshot, diagnostics);
    //     Ok(())
    // }

    #[test_case(Path::new("separate_local_folder_imports.py"))]
    fn known_local_folder(path: &Path) -> Result<()> {
        let snapshot = format!("known_local_folder_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![],
                        vec![],
                        vec!["ruff".to_string()],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_to_top.py"))]
    fn force_to_top(path: &Path) -> Result<()> {
        let snapshot = format!("force_to_top_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_to_top: BTreeSet::from([
                        "z".to_string(),
                        "lib1".to_string(),
                        "lib3".to_string(),
                        "lib5".to_string(),
                        "lib3.lib4".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("combine_as_imports.py"))]
    fn combine_as_imports(path: &Path) -> Result<()> {
        let snapshot = format!("combine_as_imports_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    combine_as_imports: true,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_wrap_aliases.py"))]
    fn force_wrap_aliases(path: &Path) -> Result<()> {
        let snapshot = format!("force_wrap_aliases_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_wrap_aliases: true,
                    combine_as_imports: true,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("magic_trailing_comma.py"))]
    fn no_split_on_trailing_comma(path: &Path) -> Result<()> {
        let snapshot = format!("split_on_trailing_comma_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    split_on_trailing_comma: false,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_single_line.py"))]
    fn force_single_line(path: &Path) -> Result<()> {
        let snapshot = format!("force_single_line_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_single_line: true,
                    single_line_exclusions: vec!["os".to_string(), "logging.handlers".to_string()]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type.py"))]
    fn order_by_type(path: &Path) -> Result<()> {
        let snapshot = format!("order_by_type_false_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    order_by_type: false,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type_with_custom_classes.py"))]
    fn order_by_type_with_custom_classes(path: &Path) -> Result<()> {
        let snapshot = format!(
            "order_by_type_with_custom_classes_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
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
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type_with_custom_constants.py"))]
    fn order_by_type_with_custom_constants(path: &Path) -> Result<()> {
        let snapshot = format!(
            "order_by_type_with_custom_constants_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
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
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("order_by_type_with_custom_variables.py"))]
    fn order_by_type_with_custom_variables(path: &Path) -> Result<()> {
        let snapshot = format!(
            "order_by_type_with_custom_variables_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
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
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_sort_within_sections.py"))]
    fn force_sort_within_sections(path: &Path) -> Result<()> {
        let snapshot = format!("force_sort_within_sections_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    force_sort_within_sections: true,
                    force_to_top: BTreeSet::from(["z".to_string()]),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring.pyi"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("multiline_docstring.py"))]
    #[test_case(Path::new("empty.py"))]
    fn required_import(path: &Path) -> Result<()> {
        let snapshot = format!("required_import_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort/required_imports").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from([
                        "from __future__ import annotations".to_string()
                    ]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring.pyi"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn required_imports(path: &Path) -> Result<()> {
        let snapshot = format!("required_imports_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort/required_imports").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
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
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring.pyi"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn combined_required_imports(path: &Path) -> Result<()> {
        let snapshot = format!("combined_required_imports_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort/required_imports").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from(["from __future__ import annotations, \
                                                       generator_stop"
                        .to_string()]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring.pyi"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("empty.py"))]
    fn straight_required_import(path: &Path) -> Result<()> {
        let snapshot = format!("straight_required_import_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort/required_imports").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from(["import os".to_string()]),
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("relative_imports_order.py"))]
    fn closest_to_furthest(path: &Path) -> Result<()> {
        let snapshot = format!("closest_to_furthest_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    relative_imports_order: RelativeImportsOrder::ClosestToFurthest,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("no_lines_before.py"))]
    fn no_lines_before(path: &Path) -> Result<()> {
        let snapshot = format!("no_lines_before.py_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    no_lines_before: BTreeSet::from([
                        ImportSection::Known(ImportType::Future),
                        ImportSection::Known(ImportType::StandardLibrary),
                        ImportSection::Known(ImportType::ThirdParty),
                        ImportSection::Known(ImportType::FirstParty),
                        ImportSection::Known(ImportType::LocalFolder),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("no_lines_before_with_empty_sections.py"))]
    fn no_lines_before_with_empty_sections(path: &Path) -> Result<()> {
        let snapshot = format!(
            "no_lines_before_with_empty_sections.py_{}",
            path.to_string_lossy()
        );
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    no_lines_before: BTreeSet::from([
                        ImportSection::Known(ImportType::StandardLibrary),
                        ImportSection::Known(ImportType::LocalFolder),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("lines_after_imports_nothing_after.py"))]
    #[test_case(Path::new("lines_after_imports_func_after.py"))]
    #[test_case(Path::new("lines_after_imports_class_after.py"))]
    fn lines_after_imports(path: &Path) -> Result<()> {
        let snapshot = format!("lines_after_imports_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    lines_after_imports: 3,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("lines_between_types.py"))]
    fn lines_between_types(path: &Path) -> Result<()> {
        let snapshot = format!("lines_between_types{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                isort: super::settings::Settings {
                    lines_between_types: 2,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(|diagnostic| diagnostic.location);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("forced_separate.py"))]
    fn forced_separate(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    forced_separate: vec![
                        // The order will be retained by the split mechanism.
                        "tests".to_string(),
                        "not_there".to_string(), // doesn't appear, shouldn't matter
                        "experiments".to_string(),
                    ],
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("sections.py"))]
    fn sections(path: &Path) -> Result<()> {
        let snapshot = format!("sections_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![],
                        vec![],
                        vec![],
                        vec![],
                        FxHashMap::from_iter([("django".to_string(), vec!["django".to_string()])]),
                    ),
                    section_order: vec![
                        ImportSection::Known(ImportType::Future),
                        ImportSection::Known(ImportType::StandardLibrary),
                        ImportSection::Known(ImportType::ThirdParty),
                        ImportSection::Known(ImportType::FirstParty),
                        ImportSection::Known(ImportType::LocalFolder),
                        ImportSection::UserDefined("django".to_string()),
                    ],

                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("sections.py"))]
    fn section_order(path: &Path) -> Result<()> {
        let snapshot = format!("section_order_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &Settings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec!["library".to_string()],
                        vec![],
                        vec![],
                        vec![],
                        FxHashMap::from_iter([("django".to_string(), vec!["django".to_string()])]),
                    ),
                    section_order: vec![
                        ImportSection::Known(ImportType::Future),
                        ImportSection::Known(ImportType::StandardLibrary),
                        ImportSection::Known(ImportType::ThirdParty),
                        ImportSection::UserDefined("django".to_string()),
                        ImportSection::Known(ImportType::FirstParty),
                        ImportSection::Known(ImportType::LocalFolder),
                    ],
                    ..super::settings::Settings::default()
                },
                ..Settings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
