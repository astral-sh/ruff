use crate::{Expr, Stmt};

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(&'a Stmt<'a>),
    Expr(&'a Expr<'a>),
}
