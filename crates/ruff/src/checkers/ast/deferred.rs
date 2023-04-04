use ruff_python_semantic::scope::ScopeStack;
use rustpython_parser::ast::{Expr, Stmt};

use ruff_python_ast::types::Range;
use ruff_python_ast::types::RefEquality;
use ruff_python_semantic::analyze::visibility::{Visibility, VisibleScope};

use crate::checkers::ast::AnnotationContext;
use crate::docstrings::definition::Definition;

type Context<'a> = (ScopeStack, Vec<RefEquality<'a, Stmt>>);

/// A collection of AST nodes that are deferred for later analysis.
/// Used to, e.g., store functions, whose bodies shouldn't be analyzed until all
/// module-level definitions have been analyzed.
#[derive(Default)]
pub struct Deferred<'a> {
    pub definitions: Vec<(Definition<'a>, Visibility, Context<'a>)>,
    pub string_type_definitions: Vec<(Range, &'a str, AnnotationContext, Context<'a>)>,
    pub type_definitions: Vec<(&'a Expr, AnnotationContext, Context<'a>)>,
    pub functions: Vec<(&'a Stmt, Context<'a>, VisibleScope)>,
    pub lambdas: Vec<(&'a Expr, Context<'a>)>,
    pub for_loops: Vec<(&'a Stmt, Context<'a>)>,
    pub assignments: Vec<Context<'a>>,
}
