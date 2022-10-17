use rustpython_ast::{Expr, Stmt};

#[derive(Debug)]
pub enum DefinitionKind<'a> {
    Module,
    Package,
    Class(&'a Stmt),
    NestedClass(&'a Stmt),
    Function(&'a Stmt),
    NestedFunction(&'a Stmt),
    Method(&'a Stmt),
}

#[derive(Debug)]
pub struct Definition<'a> {
    pub kind: DefinitionKind<'a>,
    pub docstring: Option<&'a Expr>,
}

pub enum Documentable {
    Class,
    Function,
}
