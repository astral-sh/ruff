use crate::{Expr, Stmt};

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(Stmt<'a>),
    Expr(Expr<'a>),
}
