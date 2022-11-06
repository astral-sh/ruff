use std::collections::{BTreeMap, BTreeSet};

use rustpython_ast::{Stmt, StmtKind};

use crate::autofix::{Fix, Patch};

#[derive(Debug)]
pub struct ImportTracker<'a> {
    pub blocks: Vec<Vec<&'a Stmt>>,
}

impl<'a> ImportTracker<'a> {
    pub fn new() -> Self {
        Self {
            blocks: vec![vec![]],
        }
    }

    pub fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let index = self.blocks.len() - 1;
        if matches!(
            stmt.node,
            StmtKind::Import { .. } | StmtKind::ImportFrom { .. }
        ) {
            self.blocks[index].push(stmt);
        } else {
            if !self.blocks[index].is_empty() {
                self.blocks.push(vec![]);
            }
        }
    }
}

type FromData<'a> = (&'a Option<String>, &'a Option<usize>);
type AliasData<'a> = (&'a str, &'a Option<String>);

#[derive(Debug, Default)]
pub struct ImportBlock<'a> {
    // Map from (module, level) to `AliasData`.
    import_from: BTreeMap<FromData<'a>, BTreeSet<AliasData<'a>>>,
    // Set of (name, asname).
    import: BTreeSet<AliasData<'a>>,
}

enum ImportType {
    // __future__
    Future,
    // Known standard library
    StandardLibrary,
    // Doesn't fit into any other category
    ThirdParty,
    // Local modules (but non-dotted)
    FirstParty,
    // Dot imports
    LocalFolder,
}

impl ImportType {
    fn categorize(module: &str) -> Self {
        ImportType::FirstParty
    }
}

pub fn normalize(imports: Vec<&Stmt>) -> ImportBlock {
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

fn sort_block(block: Vec<&Stmt>) -> Fix {
    // Categorize each module as: __future__, standard library, third-party, first-party.
    // Deduplicate.
    // Consolidate `from` imports under the same module.

    Fix {
        patch: Patch {
            content: "".to_string(),
            location: Default::default(),
            end_location: Default::default(),
        },
        applied: false,
    }
}
