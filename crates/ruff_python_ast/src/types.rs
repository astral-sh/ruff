use crate::flat;
use crate::{Expr, Stmt};

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(flat::View<'a, Stmt>),
    Expr(flat::View<'a, Expr>),
}
