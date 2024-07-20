use crate::{Expr, Stmt};

#[derive(Clone)]
pub enum Node<'a, 'ast> {
    Stmt(&'a Stmt<'ast>),
    Expr(&'a Expr<'ast>),
}
