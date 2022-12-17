//! Lint rules based on import analysis.

use std::path::Path;

use rustpython_parser::ast::Suite;

use crate::ast::visitor::Visitor;
use crate::checks::Check;
use crate::directives::IsortDirectives;
use crate::isort;
use crate::isort::track::ImportTracker;
use crate::settings::{flags, Settings};
use crate::source_code_locator::SourceCodeLocator;

fn check_import_blocks(
    tracker: ImportTracker,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: flags::Autofix,
    package: Option<&Path>,
) -> Vec<Check> {
    let mut checks = vec![];
    for block in tracker.into_iter() {
        if !block.imports.is_empty() {
            if let Some(check) =
                isort::plugins::check_imports(&block, locator, settings, autofix, package)
            {
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
    autofix: flags::Autofix,
    path: &Path,
    package: Option<&Path>,
) -> Vec<Check> {
    let mut tracker = ImportTracker::new(locator, directives, path);
    for stmt in python_ast {
        tracker.visit_stmt(stmt);
    }
    check_import_blocks(tracker, locator, settings, autofix, package)
}
