use ruff_text_size::TextRange;

use crate::visitor::transformer::{walk_expr, walk_keyword, Transformer};
use crate::{nodes, Expr, Keyword};

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
            Expr::BoolOp(nodes::ExprBoolOp { range, .. }) => {
                *range = self.range;
            }
            Expr::Named(nodes::ExprNamed { range, .. }) => {
                *range = self.range;
            }
            Expr::BinOp(nodes::ExprBinOp { range, .. }) => {
                *range = self.range;
            }
            Expr::UnaryOp(nodes::ExprUnaryOp { range, .. }) => {
                *range = self.range;
            }
            Expr::Lambda(nodes::ExprLambda { range, .. }) => {
                *range = self.range;
            }
            Expr::If(nodes::ExprIf { range, .. }) => {
                *range = self.range;
            }
            Expr::Dict(nodes::ExprDict { range, .. }) => {
                *range = self.range;
            }
            Expr::Set(nodes::ExprSet { range, .. }) => {
                *range = self.range;
            }
            Expr::ListComp(nodes::ExprListComp { range, .. }) => {
                *range = self.range;
            }
            Expr::SetComp(nodes::ExprSetComp { range, .. }) => {
                *range = self.range;
            }
            Expr::DictComp(nodes::ExprDictComp { range, .. }) => {
                *range = self.range;
            }
            Expr::Generator(nodes::ExprGenerator { range, .. }) => {
                *range = self.range;
            }
            Expr::Await(nodes::ExprAwait { range, .. }) => {
                *range = self.range;
            }
            Expr::Yield(nodes::ExprYield { range, .. }) => {
                *range = self.range;
            }
            Expr::YieldFrom(nodes::ExprYieldFrom { range, .. }) => {
                *range = self.range;
            }
            Expr::Compare(nodes::ExprCompare { range, .. }) => {
                *range = self.range;
            }
            Expr::Call(nodes::ExprCall { range, .. }) => {
                *range = self.range;
            }
            Expr::FString(nodes::ExprFString { range, .. }) => {
                *range = self.range;
            }
            Expr::StringLiteral(nodes::ExprStringLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::BytesLiteral(nodes::ExprBytesLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::NumberLiteral(nodes::ExprNumberLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::BooleanLiteral(nodes::ExprBooleanLiteral { range, .. }) => {
                *range = self.range;
            }
            Expr::NoneLiteral(nodes::ExprNoneLiteral { range }) => {
                *range = self.range;
            }
            Expr::EllipsisLiteral(nodes::ExprEllipsisLiteral { range }) => {
                *range = self.range;
            }
            Expr::Attribute(nodes::ExprAttribute { range, .. }) => {
                *range = self.range;
            }
            Expr::Subscript(nodes::ExprSubscript { range, .. }) => {
                *range = self.range;
            }
            Expr::Starred(nodes::ExprStarred { range, .. }) => {
                *range = self.range;
            }
            Expr::Name(nodes::ExprName { range, .. }) => {
                *range = self.range;
            }
            Expr::List(nodes::ExprList { range, .. }) => {
                *range = self.range;
            }
            Expr::Tuple(nodes::ExprTuple { range, .. }) => {
                *range = self.range;
            }
            Expr::Slice(nodes::ExprSlice { range, .. }) => {
                *range = self.range;
            }
            Expr::IpyEscapeCommand(nodes::ExprIpyEscapeCommand { range, .. }) => {
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
