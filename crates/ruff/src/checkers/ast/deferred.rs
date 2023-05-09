use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_python_semantic::analyze::visibility::{Visibility, VisibleScope};
use ruff_python_semantic::node::NodeId;
use ruff_python_semantic::scope::ScopeId;

use crate::checkers::ast::AnnotationContext;
use crate::docstrings::definition::Definition;

/// A snapshot of the current scope and statement, which will be restored when visiting any
/// deferred definitions.
type Context<'a> = (ScopeId, Option<NodeId>);

/// A collection of AST nodes that are deferred for later analysis.
/// Used to, e.g., store functions, whose bodies shouldn't be analyzed until all
/// module-level definitions have been analyzed.
#[derive(Default)]
pub struct Deferred<'a> {
    pub definitions: Vec<(Definition<'a>, Visibility, Context<'a>)>,
    pub string_type_definitions: Vec<(TextRange, &'a str, AnnotationContext, Context<'a>)>,
    pub type_definitions: Vec<(&'a Expr, AnnotationContext, Context<'a>)>,
    pub functions: Vec<(Context<'a>, VisibleScope)>,
    pub lambdas: Vec<(&'a Expr, Context<'a>)>,
    pub for_loops: Vec<Context<'a>>,
    pub assignments: Vec<Context<'a>>,
}
