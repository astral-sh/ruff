use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use ropey::RopeBuilder;
use rustpython_ast::{Stmt, StmtKind};

use crate::isort::categorize::{categorize, ImportType};
use crate::isort::types::{AliasData, ImportBlock, ImportFromData, Importable};

mod categorize;
pub mod plugins;
pub mod settings;
pub mod track;
mod types;

// Hard-code four-space indentation for the imports themselves, to match Black.
const INDENT: &str = "    ";

fn normalize_imports<'a>(imports: &'a [&'a Stmt]) -> ImportBlock<'a> {
    let mut block: ImportBlock = Default::default();
    for import in imports {
        match &import.node {
            StmtKind::Import { names } => {
                for name in names {
                    block.import.insert(AliasData {
                        name: &name.node.name,
                        asname: &name.node.asname,
                    });
                }
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                let targets = block
                    .import_from
                    .entry(ImportFromData { module, level })
                    .or_default();
                for name in names {
                    targets.insert(AliasData {
                        name: &name.node.name,
                        asname: &name.node.asname,
                    });
                }
            }
            _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
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
    for alias in block.import {
        let import_type = categorize(
            &alias.module_base(),
            src,
            known_first_party,
            known_third_party,
            extra_standard_library,
        );
        block_by_type
            .entry(import_type)
            .or_default()
            .import
            .insert(alias);
    }
    // Categorize `StmtKind::ImportFrom`.
    for (import_from, aliases) in block.import_from {
        let classification = categorize(
            &import_from.module_base(),
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
    block_by_type
}

pub fn sort_imports(
    block: Vec<&Stmt>,
    line_length: &usize,
    src: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> String {
    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(&block);

    // Categorize by type (e.g., first-party vs. third-party).
    let block_by_type = categorize_imports(
        block,
        src,
        known_first_party,
        known_third_party,
        extra_standard_library,
    );

    // Generate replacement source code.
    let mut output = RopeBuilder::new();
    let mut first_block = true;
    for import_type in [
        ImportType::Future,
        ImportType::StandardLibrary,
        ImportType::ThirdParty,
        ImportType::FirstParty,
    ] {
        if let Some(import_block) = block_by_type.get(&import_type) {
            // Add a blank line between every section.
            if !first_block {
                output.append("\n");
            } else {
                first_block = false;
            }

            // Format `StmtKind::Import` statements.
            for AliasData { name, asname } in import_block.import.iter() {
                if let Some(asname) = asname {
                    output.append(&format!("import {} as {}\n", name, asname));
                } else {
                    output.append(&format!("import {}\n", name));
                }
            }

            // Format `StmtKind::ImportFrom` statements.
            for (import_from, aliases) in import_block.import_from.iter() {
                let prelude: String = format!("from {} import ", import_from.module_name());
                let members: Vec<String> = aliases
                    .iter()
                    .map(|AliasData { name, asname }| {
                        if let Some(asname) = asname {
                            format!("{} as {}", name, asname)
                        } else {
                            name.to_string()
                        }
                    })
                    .collect();

                // Can we fit the import on a single line?
                let expected_len: usize =
                    // `from base import `
                    prelude.len()
                        // `member( as alias)?`
                        + members.iter().map(|part| part.len()).sum::<usize>()
                        // `, `
                        + 2 * (members.len() - 1);

                if expected_len <= *line_length {
                    // `from base import `
                    output.append(&prelude);
                    // `member( as alias)?(, )?`
                    for (index, part) in members.into_iter().enumerate() {
                        if index > 0 {
                            output.append(", ");
                        }
                        output.append(&part);
                    }
                    // `\n`
                    output.append("\n");
                } else {
                    // `from base import (\n`
                    output.append(&prelude);
                    output.append("(");
                    output.append("\n");

                    // `    member( as alias)?,\n`
                    for part in members {
                        output.append(INDENT);
                        output.append(&part);
                        output.append(",");
                        output.append("\n");
                    }

                    // `)\n`
                    output.append(")");
                    output.append("\n");
                }
            }
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

    #[test_case(Path::new("reorder_within_section.py"))]
    #[test_case(Path::new("no_reorder_within_section.py"))]
    #[test_case(Path::new("separate_future_imports.py"))]
    #[test_case(Path::new("separate_third_party_imports.py"))]
    #[test_case(Path::new("separate_first_party_imports.py"))]
    #[test_case(Path::new("deduplicate_imports.py"))]
    #[test_case(Path::new("combine_import_froms.py"))]
    #[test_case(Path::new("preserve_indentation.py"))]
    #[test_case(Path::new("fit_line_length.py"))]
    #[test_case(Path::new("import_from_after_import.py"))]
    #[test_case(Path::new("leading_prefix.py"))]
    #[test_case(Path::new("trailing_suffix.py"))]
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
