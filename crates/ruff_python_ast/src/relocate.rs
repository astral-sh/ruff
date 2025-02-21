use ruff_text_size::TextRange;

use crate::visitor::transformer::{walk_expr, walk_keyword, Transformer};
use crate::{self as ast};
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
            Expr::BoolOp(ast::ExprBoolOp { range, .. }) => {
                *range = self.range;
            }
            Expr::Named(ast::ExprNamed { range, .. }) => {
                *range = self.range;
            }
            Expr::BinOp(ast::ExprBinOp { range, .. }) => {
                *range = self.range;
            }
            Expr::UnaryOp(ast::ExprUnaryOp { range, .. }) => {
                *range = self.range;
            }
            Expr::Lambda(ast::ExprLambda { range, .. }) => {
                *range = self.range;
            }
            Expr::If(ast::ExprIf { range, .. }) => {
                *range = self.range;
            }
            Expr::Dict(ast::ExprDict { range, .. }) => {
                *range = self.range;
            }
            Expr::Set(ast::ExprSet { range, .. }) => {
                *range = self.range;
            }
            Expr::ListComp(ast::ExprListComp { range, .. }) => {
                *range = self.range;
            }
            Expr::SetComp(ast::ExprSetComp { range, .. }) => {
                *range = self.range;
            }
            Expr::DictComp(ast::ExprDictComp { range, .. }) => {
                *range = self.range;
            }
            Expr::Generator(ast::ExprGenerator { range, .. }) => {
                *range = self.range;
            }
            Expr::Await(ast::ExprAwait { range, .. }) => {
                *range = self.range;
            }
            Expr::Yield(ast::ExprYield { range, .. }) => {
                *range = self.range;
            }
            Expr::YieldFrom(ast::ExprYieldFrom { range, .. }) => {
                *range = self.range;
            }
            Expr::Compare(ast::ExprCompare { range, .. }) => {
                *range = self.range;
            }
            Expr::Call(ast::ExprCall { range, .. }) => {
                *range = self.range;
            }
            Expr::FString(ast::ExprFString { range, .. }) => {
                *range = self.range;
            }
            Expr::StringLiteral(ast::ExprStringLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::BytesLiteral(ast::ExprBytesLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::NumberLiteral(ast::ExprNumberLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::BooleanLiteral(ast::ExprBooleanLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::NoneLiteral(ast::ExprNoneLiteral { range }) => {
                *range = self.range;
            }
            Expr::EllipsisLiteral(ast::ExprEllipsisLiteral { range }) => {
                *range = self.range;
            }
            Expr::Attribute(ast::ExprAttribute { range, .. }) => {
                *range = self.range;
            }
            Expr::Subscript(ast::ExprSubscript { range, .. }) => {
                *range = self.range;
            }
            Expr::Starred(ast::ExprStarred { range, .. }) => {
                *range = self.range;
            }
            Expr::Name(ast::ExprName { range, .. }) => {
                *range = self.range;
            }
            Expr::List(ast::ExprList { range, .. }) => {
                *range = self.range;
            }
            Expr::Tuple(ast::ExprTuple { range, .. }) => {
                *range = self.range;
            }
            Expr::Slice(ast::ExprSlice { range, .. }) => {
                *range = self.range;
            }
            Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand { range, .. }) => {
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
