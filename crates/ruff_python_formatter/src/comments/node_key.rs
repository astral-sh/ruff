use ruff_python_ast::node::AnyNodeRef;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;

/// Used as key into the [`MultiMap`] storing the comments per node by [`Comments`].
///
/// Implements equality and hashing based on the address of the [`AnyNodeRef`] to get fast and cheap
/// hashing/equality comparison.
#[derive(Copy, Clone)]
pub(super) struct NodeRefEqualityKey<'a> {
    node: AnyNodeRef<'a>,
}

impl<'a> NodeRefEqualityKey<'a> {
    /// Creates a key for a node reference.
    pub(super) const fn from_ref(node: AnyNodeRef<'a>) -> Self {
        Self { node }
    }

    /// Returns the underlying node.
    pub(super) fn node(&self) -> AnyNodeRef {
        self.node
    }

    fn ptr(self) -> NonNull<()> {
        match self.node {
            AnyNodeRef::ModModule(node) => NonNull::from(node).cast(),
            AnyNodeRef::ModInteractive(node) => NonNull::from(node).cast(),
            AnyNodeRef::ModExpression(node) => NonNull::from(node).cast(),
            AnyNodeRef::ModFunctionType(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtFunctionDef(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAsyncFunctionDef(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtClassDef(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtReturn(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtDelete(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAssign(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAugAssign(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAnnAssign(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtFor(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAsyncFor(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtWhile(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtIf(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtWith(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAsyncWith(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtMatch(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtRaise(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtTry(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtTryStar(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtAssert(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtImport(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtImportFrom(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtGlobal(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtNonlocal(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtExpr(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtPass(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtBreak(node) => NonNull::from(node).cast(),
            AnyNodeRef::StmtContinue(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprBoolOp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprNamedExpr(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprBinOp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprUnaryOp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprLambda(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprIfExp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprDict(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprSet(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprListComp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprSetComp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprDictComp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprGeneratorExp(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprAwait(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprYield(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprYieldFrom(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprCompare(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprCall(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprFormattedValue(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprJoinedStr(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprConstant(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprAttribute(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprSubscript(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprStarred(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprName(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprList(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprTuple(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExprSlice(node) => NonNull::from(node).cast(),
            AnyNodeRef::ExcepthandlerExceptHandler(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchValue(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchSingleton(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchSequence(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchMapping(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchClass(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchStar(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchAs(node) => NonNull::from(node).cast(),
            AnyNodeRef::PatternMatchOr(node) => NonNull::from(node).cast(),
            AnyNodeRef::TypeIgnoreTypeIgnore(node) => NonNull::from(node).cast(),
            AnyNodeRef::Comprehension(node) => NonNull::from(node).cast(),
            AnyNodeRef::Arguments(node) => NonNull::from(node).cast(),
            AnyNodeRef::Arg(node) => NonNull::from(node).cast(),
            AnyNodeRef::Keyword(node) => NonNull::from(node).cast(),
            AnyNodeRef::Alias(node) => NonNull::from(node).cast(),
            AnyNodeRef::Withitem(node) => NonNull::from(node).cast(),
            AnyNodeRef::MatchCase(node) => NonNull::from(node).cast(),
        }
    }
}

impl Debug for NodeRefEqualityKey<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.node.fmt(f)
    }
}

impl PartialEq for NodeRefEqualityKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr().eq(&other.ptr())
    }
}

impl Eq for NodeRefEqualityKey<'_> {}

impl Hash for NodeRefEqualityKey<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ptr().hash(state);
    }
}

impl<'a> From<AnyNodeRef<'a>> for NodeRefEqualityKey<'a> {
    fn from(value: AnyNodeRef<'a>) -> Self {
        NodeRefEqualityKey::from_ref(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::comments::node_key::NodeRefEqualityKey;
    use ruff_python_ast::node::AnyNodeRef;
    use ruff_text_size::TextRange;
    use rustpython_parser::ast::StmtContinue;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash(key: NodeRefEqualityKey) -> u64 {
        let mut h = DefaultHasher::default();
        key.hash(&mut h);
        h.finish()
    }

    #[test]
    fn equality() {
        let continue_statement = StmtContinue {
            range: TextRange::default(),
        };

        let ref_a = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));
        let ref_b = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));

        assert_eq!(ref_a, ref_b);
        assert_eq!(hash(ref_a), hash(ref_b));
    }

    #[test]
    fn inequality() {
        let continue_statement = StmtContinue {
            range: TextRange::default(),
        };

        let boxed = Box::new(continue_statement.clone());

        let ref_a = NodeRefEqualityKey::from_ref(AnyNodeRef::from(&continue_statement));
        let ref_b = NodeRefEqualityKey::from_ref(AnyNodeRef::from(boxed.as_ref()));

        assert_ne!(ref_a, ref_b);
        assert_ne!(hash(ref_a), hash(ref_b));
    }
}
