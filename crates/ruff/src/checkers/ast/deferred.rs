use rustpython_parser::ast::{Expr, Stmt};

use crate::ast::types::RefEquality;
use crate::checkers::ast::AnnotationContext;
use crate::docstrings::definition::Definition;
use crate::visibility::{Visibility, VisibleScope};
use crate::Range;

type Context<'a> = (Vec<usize>, Vec<RefEquality<'a, Stmt>>);

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
