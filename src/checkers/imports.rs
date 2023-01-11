//! Lint rules based on import analysis.

use std::path::Path;

use rustpython_parser::ast::Suite;

use crate::ast::visitor::Visitor;
use crate::directives::IsortDirectives;
use crate::isort;
use crate::isort::track::{Block, ImportTracker};
use crate::registry::{Diagnostic, RuleCode};
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;
use crate::source_code_style::SourceCodeStyleDetector;

#[allow(clippy::too_many_arguments)]
pub fn check_imports(
    python_ast: &Suite,
    locator: &SourceCodeLocator,
    directives: &IsortDirectives,
    settings: &Settings,
    stylist: &SourceCodeStyleDetector,
    autofix: flags::Autofix,
    path: &Path,
    package: Option<&Path>,
) -> Vec<Diagnostic> {
    // Extract all imports from the AST.
    let tracker = {
        let mut tracker = ImportTracker::new(locator, directives, path);
        for stmt in python_ast {
            tracker.visit_stmt(stmt);
        }
        tracker
    };
    let blocks: Vec<&Block> = tracker.iter().collect();

    // Enforce import rules.
    let mut diagnostics = vec![];
    if settings.enabled.contains(&RuleCode::I001) {
        for block in &blocks {
            if !block.imports.is_empty() {
                if let Some(diagnostic) = isort::rules::organize_imports(
                    block, locator, settings, stylist, autofix, package,
                ) {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    if settings.enabled.contains(&RuleCode::I002) {
        diagnostics.extend(isort::rules::add_required_imports(
            &blocks, python_ast, locator, settings, autofix,
        ));
    }
    diagnostics
}
