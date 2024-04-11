use std::fmt::Formatter;
use std::marker::PhantomData;

use rustc_hash::FxHashMap;

use ruff_index::{Idx, IndexVec};
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::PreorderVisitor;
use ruff_python_ast::{
    AstNode, ModModule, NodeKind, Parameter, Stmt, StmtAnnAssign, StmtAssign, StmtAugAssign,
    StmtClassDef, StmtFunctionDef, StmtImport, StmtImportFrom, StmtTypeAlias, TypeParam,
    TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
};
use ruff_text_size::TextRange;

#[ruff_index::newtype_index]
pub struct AstId;

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct FileAstId<N: HasAstId> {
    ast_id: AstId,
    _marker: PhantomData<fn() -> N>,
}

impl<N: HasAstId> FileAstId<N> {
    pub fn upcast<M: HasAstId>(self) -> FileAstId<M>
    where
        N: Into<M>,
    {
        FileAstId {
            ast_id: self.ast_id,
            _marker: PhantomData,
        }
    }
}

impl<N: HasAstId> Copy for FileAstId<N> {}
impl<N: HasAstId> Clone for FileAstId<N> {
    fn clone(&self) -> Self {
        *self
    }
}

pub struct AstIds {
    ids: IndexVec<AstId, NodeKey>,
    reverse: FxHashMap<NodeKey, AstId>,
}

impl AstIds {
    // TODO rust analyzer doesn't allocate an ID for every node. It only allocates ids for
    //  nodes with a corresponding HIR element, that is nodes that are definitions.
    pub fn from_module(module: &ModModule) -> Self {
        let mut visitor = AstIdsVisitor::default();

        // TODO: visit_module?
        // Make sure we visit the root
        visitor.crate_id(module);
        visitor.visit_body(&module.body);

        while let Some(deferred) = visitor.deferred.pop() {
            match deferred {
                DeferredNode::FunctionDefinition(def) => {
                    def.visit_preorder(&mut visitor);
                }
                DeferredNode::ClassDefinition(def) => def.visit_preorder(&mut visitor),
            }
        }

        AstIds {
            ids: visitor.ids,
            reverse: visitor.reverse,
        }
    }

    pub fn get(&self, node: &NodeKey) -> Option<AstId> {
        self.reverse.get(node).copied()
    }

    pub fn root(&self) -> NodeKey {
        self.ids[AstId::new(0)]
    }

    // TODO: Limit this API to only nodes that have an AstId (marker trait?)
    pub fn ast_id<N: HasAstId>(&self, node: N) -> FileAstId<N> {
        let key = node.node_key();
        FileAstId {
            ast_id: self.reverse.get(&key).copied().unwrap(),
            _marker: PhantomData,
        }
    }
}

impl std::fmt::Debug for AstIds {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (key, value) in self.ids.iter_enumerated() {
            map.entry(&key, &value);
        }

        map.finish()
    }
}

impl PartialEq for AstIds {
    fn eq(&self, other: &Self) -> bool {
        self.ids == other.ids
    }
}

impl Eq for AstIds {}

#[derive(Default)]
struct AstIdsVisitor<'a> {
    ids: IndexVec<AstId, NodeKey>,
    reverse: FxHashMap<NodeKey, AstId>,
    deferred: Vec<DeferredNode<'a>>,
}

impl<'a> AstIdsVisitor<'a> {
    fn crate_id<A: HasAstId>(&mut self, node: &A) {
        let node_key = node.node_key();

        let id = self.ids.push(node_key);
        self.reverse.insert(node_key, id);
    }
}

impl<'a> PreorderVisitor<'a> for AstIdsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => {
                self.deferred.push(DeferredNode::FunctionDefinition(def));
                return;
            }
            // TODO defer visiting the assignment body, type alias parameters etc?
            Stmt::ClassDef(def) => {
                self.deferred.push(DeferredNode::ClassDefinition(def));
                return;
            }
            Stmt::Expr(_) => {
                // Skip
                return;
            }
            Stmt::Return(_) => {}
            Stmt::Delete(_) => {}
            Stmt::Assign(assignment) => self.crate_id(assignment),
            Stmt::AugAssign(assignment) => {
                self.crate_id(assignment);
            }
            Stmt::AnnAssign(assignment) => self.crate_id(assignment),
            Stmt::TypeAlias(assignment) => self.crate_id(assignment),
            Stmt::For(_) => {}
            Stmt::While(_) => {}
            Stmt::If(_) => {}
            Stmt::With(_) => {}
            Stmt::Match(_) => {}
            Stmt::Raise(_) => {}
            Stmt::Try(_) => {}
            Stmt::Assert(_) => {}
            Stmt::Import(import) => self.crate_id(import),
            Stmt::ImportFrom(import_from) => self.crate_id(import_from),
            Stmt::Global(_) => {}
            Stmt::Nonlocal(_) => {}
            Stmt::Pass(_) => {}
            Stmt::Break(_) => {}
            Stmt::Continue(_) => {}
            Stmt::IpyEscapeCommand(_) => {}
        }

        preorder::walk_stmt(self, stmt);
    }
}

enum DeferredNode<'a> {
    FunctionDefinition(&'a StmtFunctionDef),
    ClassDefinition(&'a StmtClassDef),
}

// TODO an alternative to this is to have a `NodeId` on each node (in increasing order depending on the position).
//  This would allow to reduce the size of this to a u32.
//  What would be nice if we could use an `Arc::weak_ref` here but that only works if we use
//   `Arc` internally
// TODO: Implement the logic to resolve a node, given a db (and the correct file).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct NodeKey {
    kind: NodeKind,
    range: TextRange,
}

/// Marker trait implemented by AST nodes for which we extract the `AstId`.
pub trait HasAstId: AstNode {
    fn node_key(&self) -> NodeKey {
        NodeKey {
            kind: self.as_any_node_ref().kind(),
            range: self.range(),
        }
    }
}

impl HasAstId for StmtFunctionDef {}
impl HasAstId for StmtClassDef {}
impl HasAstId for StmtAnnAssign {}
impl HasAstId for StmtAugAssign {}
impl HasAstId for StmtAssign {}
impl HasAstId for StmtTypeAlias {}

impl HasAstId for ModModule {}

impl HasAstId for StmtImport {}

impl HasAstId for StmtImportFrom {}

impl HasAstId for Parameter {}

impl HasAstId for TypeParam {}
impl HasAstId for Stmt {}
impl HasAstId for TypeParamTypeVar {}
impl HasAstId for TypeParamTypeVarTuple {}
impl HasAstId for TypeParamParamSpec {}
