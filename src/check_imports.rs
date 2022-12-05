//! Lint rules based on import analysis.

use rustpython_parser::ast::Suite;

use crate::ast::visitor::Visitor;
use crate::checks::Check;
use crate::directives::IsortDirectives;
use crate::isort;
use crate::isort::track::ImportTracker;
use crate::settings::Settings;
use crate::source_code_locator::SourceCodeLocator;

fn check_import_blocks(
    tracker: ImportTracker,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: bool,
) -> Vec<Check> {
    let mut checks = vec![];
    for block in tracker.into_iter() {
        if !block.imports.is_empty() {
            if let Some(check) = isort::plugins::check_imports(&block, locator, settings, autofix) {
                checks.push(check);
            }
        }
    }
    checks
}

pub fn check_imports(
    python_ast: &Suite,
    locator: &SourceCodeLocator,
    directives: &IsortDirectives,
    settings: &Settings,
    autofix: bool,
) -> Vec<Check> {
    let mut tracker = ImportTracker::new(directives);
    for stmt in python_ast {
        tracker.visit_stmt(stmt);
    }
    check_import_blocks(tracker, locator, settings, autofix)
}
