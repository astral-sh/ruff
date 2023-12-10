//! Lint rules based on import analysis.
use std::path::Path;

use ruff_diagnostics::Diagnostic;
use ruff_notebook::CellOffsets;
use ruff_python_ast::helpers::to_module_path;
use ruff_python_ast::imports::{populate_module_imports, ImportMap};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{PySourceType, Suite};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_source_file::Locator;

use crate::directives::IsortDirectives;
use crate::registry::Rule;
use crate::rules::isort;
use crate::rules::isort::block::{Block, BlockBuilder};
use crate::settings::LinterSettings;

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
        populate_module_imports(&mut module_imports, &module_path, stmt);
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
    settings: &LinterSettings,
    stylist: &Stylist,
    path: &Path,
    package: Option<&Path>,
    source_type: PySourceType,
    cell_offsets: Option<&CellOffsets>,
) -> (Vec<Diagnostic>, Option<ImportMap>) {
    // Extract all import blocks from the AST.
    let tracker = {
        let mut tracker =
            BlockBuilder::new(locator, directives, source_type.is_stub(), cell_offsets);
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
                    block,
                    locator,
                    stylist,
                    indexer,
                    settings,
                    package,
                    source_type,
                ) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    if settings.rules.enabled(Rule::MissingRequiredImport) {
        diagnostics.extend(isort::rules::add_required_imports(
            python_ast,
            locator,
            stylist,
            settings,
            source_type,
        ));
    }

    // Extract import map.
    let imports = extract_import_map(path, package, &blocks);

    (diagnostics, imports)
}
