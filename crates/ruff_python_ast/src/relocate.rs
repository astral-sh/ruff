use ruff_text_size::TextRange;

use crate::visitor::transformer::{walk_expr, walk_keyword, Transformer};
use crate::{Expr, Keyword};

/// Change an expression's location (recursively) to match a desired, fixed
/// range.
pub fn relocate_expr(expr: &mut Expr, range: TextRange) {
    Relocator { range }.visit_expr(expr);
}

#[derive(Debug)]
struct Relocator {
    range: TextRange,
}

impl Transformer for Relocator {
    fn visit_expr(&self, expr: &mut Expr) {
        match expr {
            Expr::BoolOp(crate::ExprBoolOp { range, .. }) => {
                *range = self.range;
            }
            Expr::Named(crate::ExprNamed { range, .. }) => {
                *range = self.range;
            }
            Expr::BinOp(crate::ExprBinOp { range, .. }) => {
                *range = self.range;
            }
            Expr::UnaryOp(crate::ExprUnaryOp { range, .. }) => {
                *range = self.range;
            }
            Expr::Lambda(crate::ExprLambda { range, .. }) => {
                *range = self.range;
            }
            Expr::If(crate::ExprIf { range, .. }) => {
                *range = self.range;
            }
            Expr::Dict(crate::ExprDict { range, .. }) => {
                *range = self.range;
            }
            Expr::Set(crate::ExprSet { range, .. }) => {
                *range = self.range;
            }
            Expr::ListComp(crate::ExprListComp { range, .. }) => {
                *range = self.range;
            }
            Expr::SetComp(crate::ExprSetComp { range, .. }) => {
                *range = self.range;
            }
            Expr::DictComp(crate::ExprDictComp { range, .. }) => {
                *range = self.range;
            }
            Expr::Generator(crate::ExprGenerator { range, .. }) => {
                *range = self.range;
            }
            Expr::Await(crate::ExprAwait { range, .. }) => {
                *range = self.range;
            }
            Expr::Yield(crate::ExprYield { range, .. }) => {
                *range = self.range;
            }
            Expr::YieldFrom(crate::ExprYieldFrom { range, .. }) => {
                *range = self.range;
            }
            Expr::Compare(crate::ExprCompare { range, .. }) => {
                *range = self.range;
            }
            Expr::Call(crate::ExprCall { range, .. }) => {
                *range = self.range;
            }
            Expr::FString(crate::ExprFString { range, .. }) => {
                *range = self.range;
            }
            Expr::StringLiteral(crate::ExprStringLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::BytesLiteral(crate::ExprBytesLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::NumberLiteral(crate::ExprNumberLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::BooleanLiteral(crate::ExprBooleanLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::NoneLiteral(crate::ExprNoneLiteral { range }) => {
                *range = self.range;
            }
            Expr::EllipsisLiteral(crate::ExprEllipsisLiteral { range }) => {
                *range = self.range;
            }
            Expr::Attribute(crate::ExprAttribute { range, .. }) => {
                *range = self.range;
            }
            Expr::Subscript(crate::ExprSubscript { range, .. }) => {
                *range = self.range;
            }
            Expr::Starred(crate::ExprStarred { range, .. }) => {
                *range = self.range;
            }
            Expr::Name(crate::ExprName { range, .. }) => {
                *range = self.range;
            }
            Expr::List(crate::ExprList { range, .. }) => {
                *range = self.range;
            }
            Expr::Tuple(crate::ExprTuple { range, .. }) => {
                *range = self.range;
            }
            Expr::Slice(crate::ExprSlice { range, .. }) => {
                *range = self.range;
            }
            Expr::IpyEscapeCommand(crate::ExprIpyEscapeCommand { range, .. }) => {
                *range = self.range;
            }
        }
        walk_expr(self, expr);
    }

    fn visit_keyword(&self, keyword: &mut Keyword) {
        keyword.range = self.range;
        walk_keyword(self, keyword);
    }
}
