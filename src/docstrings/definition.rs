use std::borrow::Cow;

use rustpython_ast::{Expr, Stmt};

#[derive(Debug, Clone)]
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

#[derive(Debug)]
pub struct Docstring<'a> {
    pub kind: DefinitionKind<'a>,
    pub expr: &'a Expr,
    pub contents: &'a Cow<'a, str>,
    pub body: &'a str,
    pub indentation: &'a Cow<'a, str>,
}

pub enum Documentable {
    Class,
    Function,
}
