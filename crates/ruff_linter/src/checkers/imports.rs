//! Lint rules based on import analysis.

use ruff_diagnostics::Diagnostic;
use ruff_notebook::CellOffsets;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::{ModModule, PySourceType, PythonVersion};
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_parser::Parsed;

use crate::directives::IsortDirectives;
use crate::package::PackageRoot;
use crate::registry::Rule;
use crate::rules::isort;
use crate::rules::isort::block::{Block, BlockBuilder};
use crate::settings::LinterSettings;
use crate::Locator;

#[allow(clippy::too_many_arguments)]
pub(crate) fn check_imports(
    parsed: &Parsed<ModModule>,
    locator: &Locator,
    indexer: &Indexer,
    directives: &IsortDirectives,
    settings: &LinterSettings,
    stylist: &Stylist,
    package: Option<PackageRoot<'_>>,
    source_type: PySourceType,
    cell_offsets: Option<&CellOffsets>,
    target_version: PythonVersion,
) -> Vec<Diagnostic> {
    // Extract all import blocks from the AST.
    let tracker = {
        let mut tracker =
            BlockBuilder::new(locator, directives, source_type.is_stub(), cell_offsets);
        tracker.visit_body(parsed.suite());
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
                    parsed.tokens(),
                    target_version,
                ) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    if settings.rules.enabled(Rule::MissingRequiredImport) {
        diagnostics.extend(isort::rules::add_required_imports(
            parsed,
            locator,
            stylist,
            settings,
            source_type,
        ));
    }

    diagnostics
}
