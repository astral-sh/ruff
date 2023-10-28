//! Rules from [isort](https://pypi.org/project/isort/).

use std::path::{Path, PathBuf};

use annotate::annotate_imports;
use block::{Block, Trailer};
pub(crate) use categorize::categorize;
use categorize::categorize_imports;
pub use categorize::{ImportSection, ImportType};
use comments::Comment;
use normalize::normalize_imports;
use order::order_imports;
use ruff_python_ast::PySourceType;
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use settings::Settings;
use sorting::cmp_either_import;
use types::EitherImport::{Import, ImportFrom};
use types::{AliasData, EitherImport, ImportBlock, TrailingComma};

use crate::line_width::{LineLength, LineWidthBuilder};
use crate::settings::types::PythonVersion;

mod annotate;
pub(crate) mod block;
pub mod categorize;
mod comments;
mod format;
mod helpers;
mod normalize;
mod order;
pub(crate) mod rules;
pub mod settings;
mod sorting;
mod split;
mod types;

#[derive(Debug)]
pub(crate) struct AnnotatedAliasData<'a> {
    pub(crate) name: &'a str,
    pub(crate) asname: Option<&'a str>,
    pub(crate) atop: Vec<Comment<'a>>,
    pub(crate) inline: Vec<Comment<'a>>,
}

#[derive(Debug)]
pub(crate) enum AnnotatedImport<'a> {
    Import {
        names: Vec<AliasData<'a>>,
        atop: Vec<Comment<'a>>,
        inline: Vec<Comment<'a>>,
    },
    ImportFrom {
        module: Option<&'a str>,
        names: Vec<AnnotatedAliasData<'a>>,
        level: Option<u32>,
        atop: Vec<Comment<'a>>,
        inline: Vec<Comment<'a>>,
        trailing_comma: TrailingComma,
    },
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) fn format_imports(
    block: &Block,
    comments: Vec<Comment>,
    locator: &Locator,
    line_length: LineLength,
    indentation_width: LineWidthBuilder,
    stylist: &Stylist,
    src: &[PathBuf],
    package: Option<&Path>,
    source_type: PySourceType,
    target_version: PythonVersion,
    settings: &Settings,
) -> String {
    let trailer = &block.trailer;
    let block = annotate_imports(
        &block.imports,
        comments,
        locator,
        settings.split_on_trailing_comma,
        source_type,
    );

    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(block, settings);

    // Categorize imports.
    let mut output = String::new();

    for block in split::split_by_forced_separate(block, &settings.forced_separate) {
        let block_output = format_import_block(
            block,
            line_length,
            indentation_width,
            stylist,
            src,
            package,
            target_version,
            settings,
        );

        if !block_output.is_empty() && !output.is_empty() {
            // If we are about to output something, and had already
            // output a block, separate them.
            output.push_str(&stylist.line_ending());
        }
        output.push_str(block_output.as_str());
    }

    let lines_after_imports = settings.lines_after_imports;
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
    line_length: LineLength,
    indentation_width: LineWidthBuilder,
    stylist: &Stylist,
    src: &[PathBuf],
    package: Option<&Path>,
    target_version: PythonVersion,
    settings: &Settings,
) -> String {
    // Categorize by type (e.g., first-party vs. third-party).
    let mut block_by_type = categorize_imports(
        block,
        src,
        package,
        settings.detect_same_package,
        &settings.known_modules,
        target_version,
    );

    let mut output = String::new();

    // Generate replacement source code.
    let mut is_first_block = true;
    let mut pending_lines_before = false;
    for import_section in &settings.section_order {
        let import_block = block_by_type.remove(import_section);

        if !settings.no_lines_before.contains(import_section) {
            pending_lines_before = true;
        }
        let Some(import_block) = import_block else {
            continue;
        };

        let imports = order_imports(import_block, settings);

        let imports = {
            let mut imports = imports
                .import
                .into_iter()
                .map(Import)
                .chain(imports.import_from.into_iter().map(ImportFrom))
                .collect::<Vec<EitherImport>>();
            if settings.force_sort_within_sections {
                imports.sort_by(|import1, import2| cmp_either_import(import1, import2, settings));
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
        let lines_between_types = settings.lines_between_types;
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
                        indentation_width,
                        stylist,
                        settings.force_wrap_aliases,
                        is_first_statement,
                        settings.split_on_trailing_comma
                            && matches!(trailing_comma, TrailingComma::Present),
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
    use ruff_text_size::Ranged;
    use rustc_hash::FxHashMap;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::rules::isort::categorize::{ImportSection, KnownModules};
    use crate::settings::LinterSettings;
    use crate::test::{test_path, test_resource_path};

    use super::categorize::ImportType;
    use super::settings::RelativeImportsOrder;

    #[test_case(Path::new("add_newline_before_comments.py"))]
    #[test_case(Path::new("as_imports_comments.py"))]
    #[test_case(Path::new("bom_sorted.py"))]
    #[test_case(Path::new("bom_unsorted.py"))]
    #[test_case(Path::new("combine_as_imports.py"))]
    #[test_case(Path::new("combine_import_from.py"))]
    #[test_case(Path::new("comments.py"))]
    #[test_case(Path::new("deduplicate_imports.py"))]
    #[test_case(Path::new("fit_line_length.py"))]
    #[test_case(Path::new("fit_line_length_comment.py"))]
    #[test_case(Path::new("force_sort_within_sections.py"))]
    #[test_case(Path::new("force_to_top.py"))]
    #[test_case(Path::new("force_wrap_aliases.py"))]
    #[test_case(Path::new("if_elif_else.py"))]
    #[test_case(Path::new("import_from_after_import.py"))]
    #[test_case(Path::new("inline_comments.py"))]
    #[test_case(Path::new("insert_empty_lines.py"))]
    #[test_case(Path::new("insert_empty_lines.pyi"))]
    #[test_case(Path::new("isort_skip_file.py"))]
    #[test_case(Path::new("leading_prefix.py"))]
    #[test_case(Path::new("magic_trailing_comma.py"))]
    #[test_case(Path::new("match_case.py"))]
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
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    fn pattern(pattern: &str) -> glob::Pattern {
        glob::Pattern::new(pattern).unwrap()
    }

    #[test_case(Path::new("separate_subpackage_first_and_third_party_imports.py"))]
    fn separate_modules(path: &Path) -> Result<()> {
        let snapshot = format!("1_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![pattern("foo.bar"), pattern("baz")],
                        vec![pattern("foo"), pattern("__future__")],
                        vec![],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("separate_subpackage_first_and_third_party_imports.py"))]
    fn separate_modules_glob(path: &Path) -> Result<()> {
        let snapshot = format!("glob_1_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![pattern("foo.*"), pattern("baz")],
                        vec![pattern("foo"), pattern("__future__")],
                        vec![],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![pattern("foo")],
                        vec![pattern("foo.bar")],
                        vec![],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
    //         &LinterSettings {
    //             src: vec![test_resource_path("fixtures/isort")],
    //             ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![],
                        vec![],
                        vec![pattern("ruff")],
                        vec![],
                        FxHashMap::default(),
                    ),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("case_sensitive.py"))]
    fn case_sensitive(path: &Path) -> Result<()> {
        let snapshot = format!("case_sensitive_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
                isort: super::settings::Settings {
                    case_sensitive: true,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    combine_as_imports: true,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    force_wrap_aliases: true,
                    combine_as_imports: true,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    split_on_trailing_comma: false,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    force_single_line: true,
                    single_line_exclusions: vec!["os".to_string(), "logging.handlers".to_string()]
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("propagate_inline_comments.py"))]
    fn propagate_inline_comments(path: &Path) -> Result<()> {
        let snapshot = format!("propagate_inline_comments_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
                isort: super::settings::Settings {
                    force_single_line: true,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    order_by_type: false,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
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
            &LinterSettings {
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
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
            &LinterSettings {
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
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
            &LinterSettings {
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("force_sort_within_sections.py"))]
    fn force_sort_within_sections(path: &Path) -> Result<()> {
        let snapshot = format!("force_sort_within_sections_{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
                isort: super::settings::Settings {
                    force_sort_within_sections: true,
                    force_to_top: BTreeSet::from(["z".to_string()]),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("comment.py"))]
    #[test_case(Path::new("comments_and_newlines.py"))]
    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring.pyi"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("docstring_with_continuation.py"))]
    #[test_case(Path::new("docstring_with_semicolon.py"))]
    #[test_case(Path::new("empty.py"))]
    #[test_case(Path::new("existing_import.py"))]
    #[test_case(Path::new("multiline_docstring.py"))]
    #[test_case(Path::new("off.py"))]
    fn required_import(path: &Path) -> Result<()> {
        let snapshot = format!("required_import_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort/required_imports").join(path).as_path(),
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from([
                        "from __future__ import annotations".to_string()
                    ]),
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::MissingRequiredImport)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("comment.py"))]
    #[test_case(Path::new("comments_and_newlines.py"))]
    #[test_case(Path::new("docstring.py"))]
    #[test_case(Path::new("docstring.pyi"))]
    #[test_case(Path::new("docstring_only.py"))]
    #[test_case(Path::new("docstring_with_continuation.py"))]
    #[test_case(Path::new("docstring_with_semicolon.py"))]
    #[test_case(Path::new("empty.py"))]
    #[test_case(Path::new("existing_import.py"))]
    #[test_case(Path::new("multiline_docstring.py"))]
    #[test_case(Path::new("off.py"))]
    fn required_import_with_alias(path: &Path) -> Result<()> {
        let snapshot = format!("required_import_with_alias_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort/required_imports").join(path).as_path(),
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from([
                        "from __future__ import annotations as _annotations".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::MissingRequiredImport)
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
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from([
                        "from __future__ import annotations".to_string(),
                        "from __future__ import generator_stop".to_string(),
                    ]),
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::MissingRequiredImport)
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
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from(["from __future__ import annotations, \
                                                       generator_stop"
                        .to_string()]),
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::MissingRequiredImport)
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
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    required_imports: BTreeSet::from(["import os".to_string()]),
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::MissingRequiredImport)
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
            &LinterSettings {
                isort: super::settings::Settings {
                    relative_imports_order: RelativeImportsOrder::ClosestToFurthest,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
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
            &LinterSettings {
                isort: super::settings::Settings {
                    no_lines_before: BTreeSet::from([
                        ImportSection::Known(ImportType::StandardLibrary),
                        ImportSection::Known(ImportType::LocalFolder),
                    ]),
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
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
            &LinterSettings {
                isort: super::settings::Settings {
                    lines_after_imports: 3,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("lines_between_types.py"))]
    fn lines_between_types(path: &Path) -> Result<()> {
        let snapshot = format!("lines_between_types{}", path.to_string_lossy());
        let mut diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
                isort: super::settings::Settings {
                    lines_between_types: 2,
                    ..super::settings::Settings::default()
                },
                src: vec![test_resource_path("fixtures/isort")],
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        diagnostics.sort_by_key(Ranged::start);
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Path::new("forced_separate.py"))]
    fn forced_separate(path: &Path) -> Result<()> {
        let snapshot = format!("{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("isort").join(path).as_path(),
            &LinterSettings {
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![],
                        vec![],
                        vec![],
                        vec![],
                        FxHashMap::from_iter([("django".to_string(), vec![pattern("django")])]),
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
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
            &LinterSettings {
                src: vec![test_resource_path("fixtures/isort")],
                isort: super::settings::Settings {
                    known_modules: KnownModules::new(
                        vec![pattern("library")],
                        vec![],
                        vec![],
                        vec![],
                        FxHashMap::from_iter([("django".to_string(), vec![pattern("django")])]),
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
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn detect_same_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("isort/detect_same_package/foo/bar.py"),
            &LinterSettings {
                src: vec![],
                isort: super::settings::Settings {
                    detect_same_package: true,
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn no_detect_same_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("isort/detect_same_package/foo/bar.py"),
            &LinterSettings {
                src: vec![],
                isort: super::settings::Settings {
                    detect_same_package: false,
                    ..super::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::UnsortedImports)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
