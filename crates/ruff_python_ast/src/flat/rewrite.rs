use super::{AstNode, BuildId, Builder, NodeType};
use crate::{Expr, InterpolatedStringElement, Stmt, StringLiteral};

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

    pub(super) fn copy_with<'ast, N: AstNode<'ast>, R: Rewriter + ?Sized>(
        &mut self,
        node: N,
        rewriter: &mut R,
    ) -> BuildId<N::Kind> {
        N::Kind::replacement(node, self, rewriter)
            .unwrap_or_else(|| self.copy_any(node.syntax_node(), rewriter).cast())
    }
}
