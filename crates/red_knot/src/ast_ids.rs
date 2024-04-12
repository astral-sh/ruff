use std::any::type_name;
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use rustc_hash::FxHashMap;

use ruff_index::{Idx, IndexVec};
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::{
    AnyNodeRef, AstNode, ExceptHandler, ExceptHandlerExceptHandler, Expr, MatchCase, ModModule,
    NodeKind, Parameter, Stmt, StmtAnnAssign, StmtAssign, StmtAugAssign, StmtClassDef,
    StmtFunctionDef, StmtGlobal, StmtImport, StmtImportFrom, StmtNonlocal, StmtTypeAlias,
    TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple, WithItem,
};
use ruff_text_size::{Ranged, TextRange};

/// A type agnostic ID that uniquely identifies an AST node in a file.
#[ruff_index::newtype_index]
pub struct AstId;

/// A typed ID that uniquely identifies an AST node in a file.
///
/// This is different from [`AstId`] in that it is a combination of ID and the type of the node the ID identifies.
/// Typing the ID prevents mixing IDs of different node types and allows to restrict the API to only accept
/// nodes for which an ID has been created (not all AST nodes get an ID).
pub struct TypedAstId<N: HasAstId> {
    erased: AstId,
    _marker: PhantomData<fn() -> N>,
}

impl<N: HasAstId> TypedAstId<N> {
    /// Upcasts this ID from a more specific node type to a more general node type.
    pub fn upcast<M: HasAstId>(self) -> TypedAstId<M>
    where
        N: Into<M>,
    {
        TypedAstId {
            erased: self.erased,
            _marker: PhantomData,
        }
    }
}

impl<N: HasAstId> Copy for TypedAstId<N> {}
impl<N: HasAstId> Clone for TypedAstId<N> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<N: HasAstId> PartialEq for TypedAstId<N> {
    fn eq(&self, other: &Self) -> bool {
        self.erased == other.erased
    }
}

impl<N: HasAstId> Eq for TypedAstId<N> {}
impl<N: HasAstId> Hash for TypedAstId<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.erased.hash(state);
    }
}

impl<N: HasAstId> Debug for TypedAstId<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TypedAstId")
            .field(&self.erased)
            .field(&type_name::<N>())
            .finish()
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
        visitor.create_id(module);
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

    /// Returns the ID to the root node.
    pub fn root(&self) -> NodeKey {
        self.ids[AstId::new(0)]
    }

    /// Returns the [`TypedAstId`] for a node.
    pub fn ast_id<N: HasAstId>(&self, node: &N) -> TypedAstId<N> {
        let key = node.syntax_node_key();
        TypedAstId {
            erased: self.reverse.get(&key).copied().unwrap(),
            _marker: PhantomData,
        }
    }

    /// Returns the [`TypedAstId`] for the node identified with the given [`TypedNodeKey`].
    pub fn ast_id_for_key<N: HasAstId>(&self, node: TypedNodeKey<N>) -> TypedAstId<N> {
        let ast_id = self.ast_id_for_node_key(node.inner);

        TypedAstId {
            erased: ast_id,
            _marker: PhantomData,
        }
    }

    /// Returns the untyped [`AstId`] for the node identified by the given `node` key.
    pub fn ast_id_for_node_key(&self, node: NodeKey) -> AstId {
        self.reverse
            .get(&node)
            .copied()
            .expect("Can't find node in AstIds map.")
    }

    /// Returns the [`TypedNodeKey`] for the node identified by the given [`TypedAstId`].
    pub fn key<N: HasAstId>(&self, id: TypedAstId<N>) -> TypedNodeKey<N> {
        let syntax_key = self.ids[id.erased];

        TypedNodeKey::new(syntax_key).unwrap()
    }

    pub fn node_key<H: HasAstId>(&self, id: TypedAstId<H>) -> NodeKey {
        self.ids[id.erased]
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
    fn create_id<A: HasAstId>(&mut self, node: &A) {
        let node_key = node.syntax_node_key();

        let id = self.ids.push(node_key);
        self.reverse.insert(node_key, id);
    }
}

impl<'a> PreorderVisitor<'a> for AstIdsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => {
                self.create_id(def);
                self.deferred.push(DeferredNode::FunctionDefinition(def));
                return;
            }
            // TODO defer visiting the assignment body, type alias parameters etc?
            Stmt::ClassDef(def) => {
                self.create_id(def);
                self.deferred.push(DeferredNode::ClassDefinition(def));
                return;
            }
            Stmt::Expr(_) => {
                // Skip
                return;
            }
            Stmt::Return(_) => {}
            Stmt::Delete(_) => {}
            Stmt::Assign(assignment) => self.create_id(assignment),
            Stmt::AugAssign(assignment) => {
                self.create_id(assignment);
            }
            Stmt::AnnAssign(assignment) => self.create_id(assignment),
            Stmt::TypeAlias(assignment) => self.create_id(assignment),
            Stmt::For(_) => {}
            Stmt::While(_) => {}
            Stmt::If(_) => {}
            Stmt::With(_) => {}
            Stmt::Match(_) => {}
            Stmt::Raise(_) => {}
            Stmt::Try(_) => {}
            Stmt::Assert(_) => {}
            Stmt::Import(import) => self.create_id(import),
            Stmt::ImportFrom(import_from) => self.create_id(import_from),
            Stmt::Global(global) => self.create_id(global),
            Stmt::Nonlocal(non_local) => self.create_id(non_local),
            Stmt::Pass(_) => {}
            Stmt::Break(_) => {}
            Stmt::Continue(_) => {}
            Stmt::IpyEscapeCommand(_) => {}
        }

        preorder::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, _expr: &'a Expr) {}

    fn visit_parameter(&mut self, parameter: &'a Parameter) {
        self.create_id(parameter);
        preorder::walk_parameter(self, parameter);
    }

    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        match except_handler {
            ExceptHandler::ExceptHandler(except_handler) => {
                self.create_id(except_handler);
            }
        }

        preorder::walk_except_handler(self, except_handler);
    }

    fn visit_with_item(&mut self, with_item: &'a WithItem) {
        self.create_id(with_item);
        preorder::walk_with_item(self, with_item);
    }

    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        self.create_id(match_case);
        preorder::walk_match_case(self, match_case);
    }

    fn visit_type_param(&mut self, type_param: &'a TypeParam) {
        self.create_id(type_param);
    }
}

enum DeferredNode<'a> {
    FunctionDefinition(&'a StmtFunctionDef),
    ClassDefinition(&'a StmtClassDef),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TypedNodeKey<N: AstNode> {
    /// The type erased node key.
    inner: NodeKey,
    _marker: PhantomData<fn() -> N>,
}

impl<N: AstNode> TypedNodeKey<N> {
    pub fn new(node_key: NodeKey) -> Option<Self> {
        N::can_cast(node_key.kind).then(|| TypedNodeKey {
            inner: node_key,
            _marker: PhantomData,
        })
    }

    pub fn resolve<'a>(&self, root: AnyNodeRef<'a>) -> Option<N::Ref<'a>> {
        let node_ref = self.inner.resolve(root)?;

        Some(N::cast_ref(node_ref).unwrap())
    }
}

struct FindNodeKeyVisitor<'a> {
    key: NodeKey,
    result: Option<AnyNodeRef<'a>>,
}

impl<'a> PreorderVisitor<'a> for FindNodeKeyVisitor<'a> {
    fn enter_node(&mut self, node: AnyNodeRef<'a>) -> TraversalSignal {
        if self.result.is_some() {
            return TraversalSignal::Skip;
        }

        if node.range() == self.key.range && node.kind() == self.key.kind {
            self.result = Some(node);
            TraversalSignal::Skip
        } else if node.range().contains_range(self.key.range) {
            TraversalSignal::Traverse
        } else {
            TraversalSignal::Skip
        }
    }

    fn visit_body(&mut self, body: &'a [Stmt]) {
        for stmt in body {
            if stmt.range().start() > self.key.range.end() {
                break;
            }

            self.visit_stmt(stmt);
        }
    }
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

impl NodeKey {
    pub fn resolve<'a>(&self, root: AnyNodeRef<'a>) -> Option<AnyNodeRef<'a>> {
        // We need to do a binary search here. Only traverse into a node if the range is withint the node
        let mut visitor = FindNodeKeyVisitor {
            key: *self,
            result: None,
        };

        if visitor.enter_node(root) == TraversalSignal::Traverse {
            root.visit_preorder(&mut visitor);
        }

        visitor.result
    }
}

/// Marker trait implemented by AST nodes for which we extract the `AstId`.
pub trait HasAstId: AstNode {
    fn node_key(&self) -> TypedNodeKey<Self>
    where
        Self: Sized,
    {
        TypedNodeKey {
            inner: self.syntax_node_key(),
            _marker: PhantomData,
        }
    }

    fn syntax_node_key(&self) -> NodeKey {
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
impl HasAstId for StmtGlobal {}
impl HasAstId for StmtNonlocal {}

impl HasAstId for ExceptHandlerExceptHandler {}
impl HasAstId for WithItem {}
impl HasAstId for MatchCase {}
