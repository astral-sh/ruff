//! Lint rules based on import analysis.

use rustpython_parser::ast::Suite;

use crate::ast::visitor::Visitor;
use crate::autofix::fixer;
use crate::checks::Check;
use crate::isort;
use crate::isort::track::ImportTracker;
use crate::settings::Settings;
use crate::source_code_locator::SourceCodeLocator;

fn check_import_blocks(
    tracker: &mut ImportTracker,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: &fixer::Mode,
) -> Vec<Check> {
    let mut checks = vec![];
    while let Some(block) = tracker.next() {
        if !block.is_empty() {
            if let Some(check) = isort::plugins::check_imports(block, locator, settings, autofix) {
                checks.push(check);
            }
        }
    }
    checks
}

pub fn check_imports(
    python_ast: &Suite,
    locator: &SourceCodeLocator,
    settings: &Settings,
    autofix: &fixer::Mode,
) -> Vec<Check> {
    let mut tracker = ImportTracker::new();
    for stmt in python_ast {
        tracker.visit_stmt(stmt);
    }
    println!("{:?}", tracker.blocks);
    check_import_blocks(&mut tracker, locator, settings, autofix)
}
