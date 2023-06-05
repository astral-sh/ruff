//! Lint rules based on import analysis.
use std::borrow::Cow;
use std::path::Path;

use rustpython_parser::ast::{self, Ranged, Stmt, Suite};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{ImportMap, ModuleImport};
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_stdlib::path::is_python_stub_file;

use crate::directives::IsortDirectives;
use crate::registry::Rule;
use crate::rules::isort;
use crate::rules::isort::block::{Block, BlockBuilder};
use crate::settings::Settings;
use crate::source_kind::SourceKind;

fn extract_import_map(path: &Path, package: Option<&Path>, blocks: &[&Block]) -> Option<ImportMap> {
    let Some(package) = package else {
        return None;
    };
    let Some(module_path) = to_module_path(package, path) else {
        return None;
    };

    let num_imports = blocks.iter().map(|block| block.imports.len()).sum();
    let mut module_imports = Vec::with_capacity(num_imports);
    for stmt in blocks.iter().flat_map(|block| &block.imports) {
        match stmt {
            Stmt::Import(ast::StmtImport { names, range: _ }) => {
                module_imports.extend(
                    names
                        .iter()
                        .map(|name| ModuleImport::new(name.name.to_string(), stmt.range())),
                );
            }
            Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range: _,
            }) => {
                let level = level.map_or(0, |level| level.to_usize());
                let module = if let Some(module) = module {
                    let module: &String = module.as_ref();
                    if level == 0 {
                        Cow::Borrowed(module)
                    } else {
                        if module_path.len() <= level {
                            continue;
                        }
                        let prefix = module_path[..module_path.len() - level].join(".");
                        Cow::Owned(format!("{prefix}.{module}"))
                    }
                } else {
                    if module_path.len() <= level {
                        continue;
                    }
                    Cow::Owned(module_path[..module_path.len() - level].join("."))
                };
                module_imports.extend(names.iter().map(|name| {
                    ModuleImport::new(format!("{}.{}", module, name.name), name.range())
                }));
            }
            _ => panic!("Expected Stmt::Import | Stmt::ImportFrom"),
        }
    }

    let mut import_map = ImportMap::default();
    import_map.insert(module_path.join("."), module_imports);
    Some(import_map)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn check_imports(
    python_ast: &Suite,
    locator: &Locator,
    indexer: &Indexer,
    directives: &IsortDirectives,
    settings: &Settings,
    stylist: &Stylist,
    path: &Path,
    package: Option<&Path>,
    source_kind: Option<&SourceKind>,
) -> (Vec<Diagnostic>, Option<ImportMap>) {
    let is_stub = is_python_stub_file(path);

    // Extract all import blocks from the AST.
    let tracker = {
        let mut tracker = BlockBuilder::new(locator, directives, is_stub, source_kind);
        tracker.visit_body(python_ast);
        tracker
    };
    let blocks: Vec<&Block> = tracker.iter().collect();

    // Enforce import rules.
    let mut diagnostics = vec![];
    if settings.rules.enabled(Rule::UnsortedImports) {
        for block in &blocks {
            if !block.imports.is_empty() {
                if let Some(diagnostic) = isort::rules::organize_imports(
                    block, locator, stylist, indexer, settings, package,
                ) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    if settings.rules.enabled(Rule::MissingRequiredImport) {
        diagnostics.extend(isort::rules::add_required_imports(
            python_ast, locator, stylist, settings, is_stub,
        ));
    }

    // Extract import map.
    let imports = extract_import_map(path, package, &blocks);

    (diagnostics, imports)
}
