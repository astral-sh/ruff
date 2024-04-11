use ruff_index::{Idx, IndexVec};
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::{
    AnyNodeRef, AstNode, ModModule, NodeKind, Stmt, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;
use std::fmt::Formatter;

#[ruff_index::newtype_index]
pub struct AstId;

// TODO THis is now something that doesn't work well with Ruff's AST because the reverse map requires lifetimes because
//  cloning the nodes would be silly.
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
        visitor.enter_node(module.into());
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
    pub fn ast_id<N: AstNode>(&self, node: N) -> AstId {
        let key = NodeKey {
            kind: node.as_any_node_ref().kind(),
            range: node.range(),
        };
        self.reverse.get(&key).copied().unwrap()
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
    fn push<A: Into<AnyNodeRef<'a>>>(&mut self, node: A) {
        let node = node.into();
        let node_key = NodeKey {
            kind: node.kind(),
            range: node.range(),
        };
        let id = self.ids.push(node_key);
        self.reverse.insert(node_key, id);
    }
}

impl<'a> PreorderVisitor<'a> for AstIdsVisitor<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        if node.is_expression() {
            return TraversalSignal::Skip;
        }

        self.push(node);
        TraversalSignal::Traverse
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => {
                self.deferred.push(DeferredNode::FunctionDefinition(def));
            }
            // TODO defer visiting the assignment body, type alias parameters etc?
            Stmt::ClassDef(def) => {
                self.deferred.push(DeferredNode::ClassDefinition(def));
            }
            Stmt::Expr(_) => {
                // Skip
            }
            _ => preorder::walk_stmt(self, stmt),
        }
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
