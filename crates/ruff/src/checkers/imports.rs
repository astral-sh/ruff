//! Lint rules based on import analysis.
use std::borrow::Cow;
use std::path::Path;

use rustpython_parser::ast::{StmtKind, Suite};

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{ImportMap, ModuleImport};
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};
use ruff_python_ast::visitor::Visitor;
use ruff_python_stdlib::path::is_python_stub_file;

use crate::directives::IsortDirectives;
use crate::registry::Rule;
use crate::rules::isort;
use crate::rules::isort::track::{Block, ImportTracker};
use crate::settings::{flags, Settings};

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
        match &stmt.node {
            StmtKind::Import { names } => {
                module_imports.extend(names.iter().map(|name| {
                    ModuleImport::new(
                        name.node.name.clone(),
                        stmt.location,
                        stmt.end_location.unwrap(),
                    )
                }));
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                let level = level.unwrap_or(0);
                let module = if let Some(module) = module {
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
                    ModuleImport::new(
                        format!("{}.{}", module, name.node.name),
                        name.location,
                        name.end_location.unwrap(),
                    )
                }));
            }
            _ => panic!("Expected StmtKind::Import | StmtKind::ImportFrom"),
        }
    }

    let mut import_map = ImportMap::default();
    import_map.insert(module_path.join("."), module_imports);
    Some(import_map)
}

#[allow(clippy::too_many_arguments)]
pub fn check_imports(
    python_ast: &Suite,
    locator: &Locator,
    indexer: &Indexer,
    directives: &IsortDirectives,
    settings: &Settings,
    stylist: &Stylist,
    autofix: flags::Autofix,
    path: &Path,
    package: Option<&Path>,
) -> (Vec<Diagnostic>, Option<ImportMap>) {
    let is_stub = is_python_stub_file(path);

    // Extract all imports from the AST.
    let tracker = {
        let mut tracker = ImportTracker::new(locator, directives, is_stub);
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
                    block, locator, stylist, indexer, settings, autofix, package,
                ) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    if settings.rules.enabled(Rule::MissingRequiredImport) {
        diagnostics.extend(isort::rules::add_required_imports(
            &blocks, python_ast, locator, stylist, settings, autofix, is_stub,
        ));
    }

    // Extract import map.
    let imports = extract_import_map(path, package, &blocks);

    (diagnostics, imports)
}
