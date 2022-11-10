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
    src_paths: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> BTreeMap<ImportType, ImportBlock<'a>> {
    let mut block_by_type: BTreeMap<ImportType, ImportBlock> = Default::default();
    // Categorize `StmtKind::Import`.
    for alias in block.import {
        let import_type = categorize(
            &alias.module_base(),
            src_paths,
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
            src_paths,
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
    src_paths: &[PathBuf],
    known_first_party: &BTreeSet<String>,
    known_third_party: &BTreeSet<String>,
    extra_standard_library: &BTreeSet<String>,
) -> String {
    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(&block);

    // Categorize by type (e.g., first-party vs. third-party).
    let block_by_type = categorize_imports(
        block,
        src_paths,
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
                // STOPSHIP(charlie): Try to squeeze into available line-length.
                output.append(&format!("from {} import (\n", import_from.module_name()));
                for AliasData { name, asname } in aliases {
                    if let Some(asname) = asname {
                        output.append(&format!("{}{} as {},\n", INDENT, name, asname));
                    } else {
                        output.append(&format!("{}{},\n", INDENT, name));
                    }
                }
                output.append(")\n");
            }
        }
    }
    output.finish().to_string()
}
