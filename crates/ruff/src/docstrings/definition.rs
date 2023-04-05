use rustpython_parser::ast::{Expr, Stmt};

use ruff_python_semantic::analyze::visibility::{
    class_visibility, function_visibility, method_visibility, Modifier, Visibility, VisibleScope,
};

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
    pub contents: &'a str,
    pub body: &'a str,
    pub indentation: &'a str,
}

#[derive(Copy, Clone)]
pub enum Documentable {
    Class,
    Function,
}

pub fn transition_scope(scope: VisibleScope, stmt: &Stmt, kind: Documentable) -> VisibleScope {
    match kind {
        Documentable::Function => VisibleScope {
            modifier: Modifier::Function,
            visibility: match scope {
                VisibleScope {
                    modifier: Modifier::Module,
                    visibility: Visibility::Public,
                } => function_visibility(stmt),
                VisibleScope {
                    modifier: Modifier::Class,
                    visibility: Visibility::Public,
                } => method_visibility(stmt),
                _ => Visibility::Private,
            },
        },
        Documentable::Class => VisibleScope {
            modifier: Modifier::Class,
            visibility: match scope {
                VisibleScope {
                    modifier: Modifier::Module,
                    visibility: Visibility::Public,
                } => class_visibility(stmt),
                VisibleScope {
                    modifier: Modifier::Class,
                    visibility: Visibility::Public,
                } => class_visibility(stmt),
                _ => Visibility::Private,
            },
        },
    }
}
