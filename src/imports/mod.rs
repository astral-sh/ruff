use std::collections::BTreeMap;

use ropey::RopeBuilder;
use rustpython_ast::{Stmt, StmtKind};

use crate::imports::categorize::{categorize, ImportType};
use crate::imports::types::ImportBlock;

mod categorize;
pub mod plugins;
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
                    block.import.insert((&name.node.name, &name.node.asname));
                }
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                let targets = block.import_from.entry((module, level)).or_default();
                for name in names {
                    targets.insert((&name.node.name, &name.node.asname));
                }
            }
            _ => unreachable!("Expected StmtKind::Import | StmtKind::ImportFrom"),
        }
    }
    block
}

fn categorize_imports(block: ImportBlock) -> BTreeMap<ImportType, ImportBlock> {
    let mut block_by_type: BTreeMap<ImportType, ImportBlock> = Default::default();
    // Categorize `StmtKind::Import`.
    for (name, asname) in block.import {
        let module_base = name.split('.').next().unwrap();
        let import_type = categorize(module_base);
        block_by_type
            .entry(import_type)
            .or_default()
            .import
            .insert((name, asname));
    }
    // Categorize `StmtKind::ImportFrom`.
    for ((module, level), aliases) in block.import_from {
        let mut module_base = String::new();
        if let Some(level) = level {
            if level > &0 {
                module_base.push_str(&".".repeat(*level));
            }
        }
        if let Some(module) = module {
            module_base.push_str(module);
        }
        let module_base = module_base.split('.').next().unwrap();
        let classification = categorize(module_base);
        block_by_type
            .entry(classification)
            .or_default()
            .import_from
            .insert((module, level), aliases);
    }
    block_by_type
}

pub fn sort_imports(block: Vec<&Stmt>, line_length: usize) -> String {
    // Normalize imports (i.e., deduplicate, aggregate `from` imports).
    let block = normalize_imports(&block);

    // Categorize by type (e.g., first-party vs. third-party).
    let block_by_type = categorize_imports(block);

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
            for (name, asname) in import_block.import.iter() {
                if let Some(asname) = asname {
                    output.append(&format!("import {} as {}\n", name, asname));
                } else {
                    output.append(&format!("import {}\n", name));
                }
            }

            // Format `StmtKind::ImportFrom` statements.
            for ((module, level), aliases) in import_block.import_from.iter() {
                // STOPSHIP(charlie): Extract this into a method.
                let mut module_base = String::new();
                if let Some(level) = level {
                    if level > &0 {
                        module_base.push_str(&".".repeat(*level));
                    }
                }
                if let Some(module) = module {
                    module_base.push_str(module);
                }
                // STOPSHIP(charlie): Try to squeeze into available line-length.
                output.append(&format!("from {} import (\n", module_base));
                for (name, asname) in aliases {
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
