#![allow(clippy::derive_partial_eq_without_eq)]
pub use crate::{builtin::*, text_size::TextSize, ConversionFlag, Node};
use std::fmt::Debug;

pub type Suite = Vec<Stmt>;

impl CmpOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        }
    }
}

impl Arguments {
    pub fn empty(range: TextRange) -> Self {
        Self {
            range,
            posonlyargs: Vec::new(),
            args: Vec::new(),
            vararg: None,
            kwonlyargs: Vec::new(),
            kwarg: None,
        }
    }
}

#[allow(clippy::borrowed_box)] // local utility
fn clone_boxed_expr(expr: &Box<Expr>) -> Box<Expr> {
    let expr: &Expr = expr.as_ref();
    Box::new(expr.clone())
}

impl ArgWithDefault {
    pub fn as_arg(&self) -> &Arg {
        &self.def
    }

    pub fn to_arg(&self) -> (Arg, Option<Box<Expr>>) {
        let ArgWithDefault {
            range: _,
            def,
            default,
        } = self;
        (def.clone(), default.as_ref().map(clone_boxed_expr))
    }
    pub fn into_arg(self) -> (Arg, Option<Box<Expr>>) {
        let ArgWithDefault {
            range: _,
            def,
            default,
        } = self;
        (def, default)
    }
}

impl Arguments {
    pub fn defaults(&self) -> impl std::iter::Iterator<Item = &Expr> {
        self.posonlyargs
            .iter()
            .chain(self.args.iter())
            .filter_map(|arg| arg.default.as_ref().map(|e| e.as_ref()))
    }

    #[allow(clippy::type_complexity)]
    pub fn split_kwonlyargs(&self) -> (Vec<&Arg>, Vec<(&Arg, &Expr)>) {
        let mut args = Vec::new();
        let mut with_defaults = Vec::new();
        for arg in self.kwonlyargs.iter() {
            if let Some(ref default) = arg.default {
                with_defaults.push((arg.as_arg(), &**default));
            } else {
                args.push(arg.as_arg());
            }
        }
        (args, with_defaults)
    }
}

include!("gen/generic.rs");
