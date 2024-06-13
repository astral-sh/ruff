// What if we do a semantic scope which is the scope
// at which we perform any semantic analysis, including
// ast ids, control flow analysis, symbol tables, type checking etc.

pub enum SemanticScope {
    Module(VfsFile),
    Function(FunctionSemanticScope),
    Class(ClassSemanticScope),
}

// Using the nodes directly is probably bad
// for persistent caching because it means we need
// to deserialize the nodes every time we access
// the scopes, even if the scopes haven't changed?
//
// So I think we want to have an indirection here with a
// stable ID, but that id would be the same as the scope,
// which makes this a bit awkward?
//
// Let's say we have an ID. How would we build this? It would require
// a full AST traversal just to get the scope IDs,
// then another traversal to get the AST IDs per scope
// But we could build these together with the semantic index?
//
// The other problem is how do we get from expression to the semantic scope?
// The challenge is that expression starts bottom up and not top down

// What if we have something like an owner struct
// where the owner stores the file and the parent
// we then need a single lookup table from `expression` to owner.
// In the end, this is kind of what `Scope` is.
//

// And make this a salsa ingredient.
#[salsa::tracked]
pub struct Scope {
    #[id]
    node: AstNodeId,
    file: VfsFile,
    parent: Option<ScopeId>,

    #[return_ref]
    symbol_table: SymbolTable,

    #[return_ref]
    control_flow_graph: ControlFlowGraph,
}

// This would require building scopes one by one, or at least,
// interning them one by one.
// Step one: Create AstIds.
// Step two: Create scopes. We can build them all at once but need to create
// a different `Scope` ingredient for each of them.
// What about comprehension and lambdas? Should we just
// Run inference at the same time to avoid the complexity of
// having different scope concepts? Maybe, seems like a good idea.

pub struct AstNodeRef<T> {
    node: T,
}
