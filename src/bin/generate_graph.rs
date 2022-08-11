use std::path::PathBuf;

use anyhow::Result;
use rustpython_parser::ast::{Stmt, StmtKind, Suite};
use rustpython_parser::parser;

use ::rust_python_linter::fs;
use ::rust_python_linter::visitor::{walk_stmt, Visitor};

#[allow(dead_code)]
#[derive(Debug)]
struct ModuleImport {
    module_name: Option<String>,
    remote_name: Option<String>,
    local_name: Option<String>,
    lineno: usize,
    pragma: usize,
}

#[derive(Default)]
struct ImportVisitor {
    imports: Vec<ModuleImport>,
}

// Inspired by: https://github.com/blais/snakefood/blob/f902c9a099f7c5bb75154a747bf098259211025d/lib/python/snakefood/find.py#L241
impl Visitor for ImportVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtKind::Import { names } => {
                for alias in names {
                    self.imports.push(ModuleImport {
                        module_name: Some(alias.name.clone()),
                        remote_name: None,
                        local_name: Some(
                            alias.asname.clone().unwrap_or_else(|| alias.name.clone()),
                        ),
                        lineno: stmt.location.row(),
                        pragma: 0,
                    })
                }
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                if let Some(module_name) = module {
                    if module_name == "__future__" {
                        return;
                    }
                }

                for alias in names {
                    if alias.name == "*" {
                        self.imports.push(ModuleImport {
                            module_name: module.clone(),
                            remote_name: None,
                            local_name: None,
                            lineno: stmt.location.row(),
                            pragma: *level,
                        })
                    } else {
                        self.imports.push(ModuleImport {
                            module_name: module.clone(),
                            remote_name: Some(alias.name.clone()),
                            local_name: Some(
                                alias.asname.clone().unwrap_or_else(|| alias.name.clone()),
                            ),
                            lineno: stmt.location.row(),
                            pragma: *level,
                        })
                    }
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

fn collect_imports(python_ast: &Suite) -> Vec<ModuleImport> {
    python_ast
        .iter()
        .flat_map(|stmt| {
            let mut visitor: ImportVisitor = Default::default();
            visitor.visit_stmt(stmt);
            visitor.imports
        })
        .collect()
}

fn main() -> Result<()> {
    // What else is required here? Map from modules to files.
    let files = fs::iter_python_files(&PathBuf::from("resources/test/src"));
    for entry in files {
        // Read the file from disk.
        let contents = fs::read_file(entry.path())?;

        // Run the parser.
        let python_ast = parser::parse_program(&contents)?;

        // Collect imports.
        let imports = collect_imports(&python_ast);
        for import in imports {
            println!("{} imports: {:?}", entry.path().to_string_lossy(), import)
        }
    }
    Ok(())
}
