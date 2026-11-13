use ruff_text_size::Ranged;

use super::{AstNode, BuildId, Builder, NodeType};
use crate::{Expr, ExprCompare, InterpolatedStringElement, Stmt, StringLiteral, build, kind};

/// Replaces selected subtrees while copying syntax into another arena.
///
/// Return a replacement built in the destination arena to take responsibility for
/// the whole subtree. Returning `None` copies the original node and rewrites its
/// children. Use [`Builder::copy_node`] to preserve a subtree as written.
pub trait Rewriter {
    fn replace_stmt(
        &mut self,
        _builder: &mut Builder,
        _stmt: Stmt<'_>,
    ) -> Option<BuildId<crate::kind::Stmt>> {
        None
    }

    fn replace_expr(
        &mut self,
        _builder: &mut Builder,
        _expr: Expr<'_>,
    ) -> Option<BuildId<crate::kind::Expr>> {
        None
    }

    fn replace_string_literal(
        &mut self,
        _builder: &mut Builder,
        _literal: StringLiteral<'_>,
    ) -> Option<BuildId<crate::kind::StringLiteral>> {
        None
    }

    fn replace_interpolated_string_element(
        &mut self,
        _builder: &mut Builder,
        _element: InterpolatedStringElement<'_>,
    ) -> Option<BuildId<crate::kind::InterpolatedStringElement>> {
        None
    }
}

pub(super) struct Identity;

impl Rewriter for Identity {}

impl Builder {
    /// Copies a subtree, allowing the rewriter to replace nodes of each field's
    /// declared type. An expression field can change from one expression kind to another.
    pub fn rewrite_node<'ast, N: AstNode<'ast>>(
        &mut self,
        node: N,
        rewriter: &mut dyn Rewriter,
    ) -> BuildId<N::Kind> {
        self.copy_with(node, rewriter)
    }

    pub(super) fn copy_comparison<R: Rewriter + ?Sized>(
        &mut self,
        node: ExprCompare<'_>,
        rewriter: &mut R,
    ) -> BuildId<kind::ExprCompare> {
        // Copying an operand can append its own lists. Write both comparison
        // lists after copying the children so they share a boundary.
        let start = self.scratch.len();
        for operand in node.operands() {
            let copy = self.copy_with(operand, rewriter);
            self.scratch.push(copy.slot);
        }
        let ops = self.list(node.ops());
        let operands = self.finish_list(start);
        self.add(build::ExprCompare {
            range: node.range(),
            ops,
            operands,
        })
    }

    pub(super) fn copy_with<'ast, N: AstNode<'ast>, R: Rewriter + ?Sized>(
        &mut self,
        node: N,
        rewriter: &mut R,
    ) -> BuildId<N::Kind> {
        N::Kind::replacement(node, self, rewriter)
            .unwrap_or_else(|| self.copy_any(node.syntax_node(), rewriter).cast())
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextRange;

    use super::Builder;
    use crate::{BuildId, CmpOp, ExprContext, build, kind};

    #[test]
    fn copying_comparison_keeps_operators_and_operands_adjacent() {
        let mut source = Builder::default();
        let none: BuildId<kind::Expr> = source
            .add(build::ExprNoneLiteral {
                range: TextRange::default(),
            })
            .into();
        let elts = source.list([none]);
        let list = source.add(build::ExprList {
            range: TextRange::default(),
            elts,
            ctx: ExprContext::Load,
        });
        let ops = source.list([CmpOp::In]);
        let operands = source.list([none, list.into()]);
        let comparison = source.add(build::ExprCompare {
            range: TextRange::default(),
            ops,
            operands,
        });

        let mut copy = Builder::default();
        let comparison = copy.copy_node(source.get(comparison));
        assert_eq!(copy.ast.extra.len(), 4); // One list element, one operator and two operands.
        assert_eq!(copy.get(comparison).ops().as_array(), Some([CmpOp::In]));
    }
}
