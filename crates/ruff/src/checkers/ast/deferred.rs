use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_python_semantic::context::Snapshot;

/// A collection of AST nodes that are deferred for later analysis.
/// Used to, e.g., store functions, whose bodies shouldn't be analyzed until all
/// module-level definitions have been analyzed.
#[derive(Default)]
pub struct Deferred<'a> {
    pub string_type_definitions: Vec<(TextRange, &'a str, Snapshot)>,
    pub type_definitions: Vec<(&'a Expr, Snapshot)>,
    pub functions: Vec<Snapshot>,
    pub lambdas: Vec<(&'a Expr, Snapshot)>,
    pub for_loops: Vec<Snapshot>,
    pub assignments: Vec<Snapshot>,
}
