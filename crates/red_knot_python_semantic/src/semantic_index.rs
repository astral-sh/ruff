use std::iter::FusedIterator;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_db::parsed::parsed_module;
use ruff_db::vfs::VfsFile;
use ruff_index::{IndexSlice, IndexVec};
use ruff_python_ast as ast;

use crate::node_key::NodeKey;
use crate::semantic_index::ast_ids::{AstId, AstIds, ScopedClassId, ScopedFunctionId};
use crate::semantic_index::builder::SemanticIndexBuilder;
use crate::semantic_index::symbol::{
    FileScopeId, PublicSymbolId, Scope, ScopeId, ScopedSymbolId, SymbolTable,
};
use crate::Db;

pub mod ast_ids;
mod builder;
pub mod definition;
pub mod symbol;

type SymbolMap = hashbrown::HashMap<ScopedSymbolId, (), ()>;

/// Returns the semantic index for `file`.
///
/// Prefer using [`symbol_table`] when working with symbols from a single scope.
#[salsa::tracked(return_ref, no_eq)]
pub(crate) fn semantic_index(db: &dyn Db, file: VfsFile) -> SemanticIndex {
    let _span = tracing::trace_span!("semantic_index", ?file).entered();

    let parsed = parsed_module(db.upcast(), file);

    SemanticIndexBuilder::new(parsed).build()
}

/// Returns the symbol table for a specific `scope`.
///
/// Using [`symbol_table`] over [`semantic_index`] has the advantage that
/// Salsa can avoid invalidating dependent queries if this scope's symbol table
/// is unchanged.
#[salsa::tracked]
pub(crate) fn symbol_table<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Arc<SymbolTable> {
    let _span = tracing::trace_span!("symbol_table", ?scope).entered();
    let index = semantic_index(db, scope.file(db));

    index.symbol_table(scope.file_scope_id(db))
}

/// Returns the root scope of `file`.
#[salsa::tracked]
pub(crate) fn root_scope(db: &dyn Db, file: VfsFile) -> ScopeId<'_> {
    let _span = tracing::trace_span!("root_scope", ?file).entered();

    FileScopeId::root().to_scope_id(db, file)
}

/// Returns the symbol with the given name in `file`'s public scope or `None` if
/// no symbol with the given name exists.
pub fn public_symbol<'db>(
    db: &'db dyn Db,
    file: VfsFile,
    name: &str,
) -> Option<PublicSymbolId<'db>> {
    let root_scope = root_scope(db, file);
    let symbol_table = symbol_table(db, root_scope);
    let local = symbol_table.symbol_id_by_name(name)?;
    Some(local.to_public_symbol(db, file))
}

/// The symbol tables for an entire file.
#[derive(Debug)]
pub struct SemanticIndex {
    /// List of all symbol tables in this file, indexed by scope.
    symbol_tables: IndexVec<FileScopeId, Arc<SymbolTable>>,

    /// List of all scopes in this file.
    scopes: IndexVec<FileScopeId, Scope>,

    /// Maps expressions to their corresponding scope.
    /// We can't use [`ExpressionId`] here, because the challenge is how to get from
    /// an [`ast::Expr`] to an [`ExpressionId`] (which requires knowing the scope).
    scopes_by_expression: FxHashMap<NodeKey, FileScopeId>,

    /// Lookup table to map between node ids and ast nodes.
    ///
    /// Note: We should not depend on this map when analysing other files or
    /// changing a file invalidates all dependents.
    ast_ids: IndexVec<FileScopeId, AstIds>,

    /// Map from scope to the node that introduces the scope.
    nodes_by_scope: IndexVec<FileScopeId, NodeWithScopeId>,

    /// Map from nodes that introduce a scope to the scope they define.
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,
}

impl SemanticIndex {
    /// Returns the symbol table for a specific scope.
    ///
    /// Use the Salsa cached [`symbol_table`] query if you only need the
    /// symbol table for a single scope.
    pub(super) fn symbol_table(&self, scope_id: FileScopeId) -> Arc<SymbolTable> {
        self.symbol_tables[scope_id].clone()
    }

    pub(crate) fn ast_ids(&self, scope_id: FileScopeId) -> &AstIds {
        &self.ast_ids[scope_id]
    }

    /// Returns the ID of the `expression`'s enclosing scope.
    pub(crate) fn expression_scope_id<'expr>(
        &self,
        expression: impl Into<ast::ExpressionRef<'expr>>,
    ) -> FileScopeId {
        self.scopes_by_expression[&NodeKey::from_node(expression.into())]
    }

    /// Returns the [`Scope`] of the `expression`'s enclosing scope.
    #[allow(unused)]
    pub(crate) fn expression_scope<'expr>(
        &self,
        expression: impl Into<ast::ExpressionRef<'expr>>,
    ) -> &Scope {
        &self.scopes[self.expression_scope_id(expression)]
    }

    /// Returns the [`Scope`] with the given id.
    pub(crate) fn scope(&self, id: FileScopeId) -> &Scope {
        &self.scopes[id]
    }

    /// Returns the id of the parent scope.
    pub(crate) fn parent_scope_id(&self, scope_id: FileScopeId) -> Option<FileScopeId> {
        let scope = self.scope(scope_id);
        scope.parent
    }

    /// Returns the parent scope of `scope_id`.
    #[allow(unused)]
    pub(crate) fn parent_scope(&self, scope_id: FileScopeId) -> Option<&Scope> {
        Some(&self.scopes[self.parent_scope_id(scope_id)?])
    }

    /// Returns an iterator over the descendent scopes of `scope`.
    #[allow(unused)]
    pub(crate) fn descendent_scopes(&self, scope: FileScopeId) -> DescendentsIter {
        DescendentsIter::new(self, scope)
    }

    /// Returns an iterator over the direct child scopes of `scope`.
    pub(crate) fn child_scopes(&self, scope: FileScopeId) -> ChildrenIter {
        ChildrenIter::new(self, scope)
    }

    /// Returns an iterator over all ancestors of `scope`, starting with `scope` itself.
    pub(crate) fn ancestor_scopes(&self, scope: FileScopeId) -> AncestorsIter {
        AncestorsIter::new(self, scope)
    }

    pub(crate) fn scope_node(&self, scope_id: FileScopeId) -> NodeWithScopeId {
        self.nodes_by_scope[scope_id]
    }

    pub(crate) fn definition_scope(
        &self,
        node_with_scope: impl Into<NodeWithScopeKey>,
    ) -> FileScopeId {
        self.scopes_by_node[&node_with_scope.into()]
    }
}

/// ID that uniquely identifies an expression inside a [`Scope`].

pub struct AncestorsIter<'a> {
    scopes: &'a IndexSlice<FileScopeId, Scope>,
    next_id: Option<FileScopeId>,
}

impl<'a> AncestorsIter<'a> {
    fn new(module_symbol_table: &'a SemanticIndex, start: FileScopeId) -> Self {
        Self {
            scopes: &module_symbol_table.scopes,
            next_id: Some(start),
        }
    }
}

impl<'a> Iterator for AncestorsIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let current_id = self.next_id?;
        let current = &self.scopes[current_id];
        self.next_id = current.parent;

        Some((current_id, current))
    }
}

impl FusedIterator for AncestorsIter<'_> {}

pub struct DescendentsIter<'a> {
    next_id: FileScopeId,
    descendents: std::slice::Iter<'a, Scope>,
}

impl<'a> DescendentsIter<'a> {
    fn new(symbol_table: &'a SemanticIndex, scope_id: FileScopeId) -> Self {
        let scope = &symbol_table.scopes[scope_id];
        let scopes = &symbol_table.scopes[scope.descendents.clone()];

        Self {
            next_id: scope_id + 1,
            descendents: scopes.iter(),
        }
    }
}

impl<'a> Iterator for DescendentsIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        let descendent = self.descendents.next()?;
        let id = self.next_id;
        self.next_id = self.next_id + 1;

        Some((id, descendent))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.descendents.size_hint()
    }
}

impl FusedIterator for DescendentsIter<'_> {}

impl ExactSizeIterator for DescendentsIter<'_> {}

pub struct ChildrenIter<'a> {
    parent: FileScopeId,
    descendents: DescendentsIter<'a>,
}

impl<'a> ChildrenIter<'a> {
    fn new(module_symbol_table: &'a SemanticIndex, parent: FileScopeId) -> Self {
        let descendents = DescendentsIter::new(module_symbol_table, parent);

        Self {
            parent,
            descendents,
        }
    }
}

impl<'a> Iterator for ChildrenIter<'a> {
    type Item = (FileScopeId, &'a Scope);

    fn next(&mut self) -> Option<Self::Item> {
        self.descendents
            .find(|(_, scope)| scope.parent == Some(self.parent))
    }
}

impl FusedIterator for ChildrenIter<'_> {}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum NodeWithScopeId {
    Module,
    Class(AstId<ScopedClassId>),
    ClassTypeParams(AstId<ScopedClassId>),
    Function(AstId<ScopedFunctionId>),
    FunctionTypeParams(AstId<ScopedFunctionId>),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct NodeWithScopeKey(NodeKey);

impl From<&ast::StmtClassDef> for NodeWithScopeKey {
    fn from(node: &ast::StmtClassDef) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl From<&ast::StmtFunctionDef> for NodeWithScopeKey {
    fn from(value: &ast::StmtFunctionDef) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::TypeParams> for NodeWithScopeKey {
    fn from(value: &ast::TypeParams) -> Self {
        Self(NodeKey::from_node(value))
    }
}

impl From<&ast::ModModule> for NodeWithScopeKey {
    fn from(value: &ast::ModModule) -> Self {
        Self(NodeKey::from_node(value))
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::parsed::parsed_module;
    use ruff_db::vfs::{system_path_to_file, VfsFile};

    use crate::db::tests::TestDb;
    use crate::semantic_index::symbol::{FileScopeId, ScopeKind, SymbolTable};
    use crate::semantic_index::{root_scope, semantic_index, symbol_table};

    struct TestCase {
        db: TestDb,
        file: VfsFile,
    }

    fn test_case(content: impl ToString) -> TestCase {
        let db = TestDb::new();
        db.memory_file_system()
            .write_file("test.py", content)
            .unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();

        TestCase { db, file }
    }

    fn names(table: &SymbolTable) -> Vec<&str> {
        table
            .symbols()
            .map(|symbol| symbol.name().as_str())
            .collect()
    }

    #[test]
    fn empty() {
        let TestCase { db, file } = test_case("");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), Vec::<&str>::new());
    }

    #[test]
    fn simple() {
        let TestCase { db, file } = test_case("x");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["x"]);
    }

    #[test]
    fn annotation_only() {
        let TestCase { db, file } = test_case("x: int");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["int", "x"]);
        // TODO record definition
    }

    #[test]
    fn import() {
        let TestCase { db, file } = test_case("import foo");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["foo"]);
        let foo = root_table.symbol_by_name("foo").unwrap();

        assert_eq!(foo.definitions().len(), 1);
    }

    #[test]
    fn import_sub() {
        let TestCase { db, file } = test_case("import foo.bar");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["foo"]);
    }

    #[test]
    fn import_as() {
        let TestCase { db, file } = test_case("import foo.bar as baz");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["baz"]);
    }

    #[test]
    fn import_from() {
        let TestCase { db, file } = test_case("from bar import foo");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["foo"]);
        assert_eq!(
            root_table
                .symbol_by_name("foo")
                .unwrap()
                .definitions()
                .len(),
            1
        );
        assert!(
            root_table
                .symbol_by_name("foo")
                .is_some_and(|symbol| { symbol.is_defined() || !symbol.is_used() }),
            "symbols that are defined get the defined flag"
        );
    }

    #[test]
    fn assign() {
        let TestCase { db, file } = test_case("x = foo");
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["foo", "x"]);
        assert_eq!(
            root_table.symbol_by_name("x").unwrap().definitions().len(),
            1
        );
        assert!(
            root_table
                .symbol_by_name("foo")
                .is_some_and(|symbol| { !symbol.is_defined() && symbol.is_used() }),
            "a symbol used but not defined in a scope should have only the used flag"
        );
    }

    #[test]
    fn class_scope() {
        let TestCase { db, file } = test_case(
            "
class C:
    x = 1
y = 2
",
        );
        let root_table = symbol_table(&db, root_scope(&db, file));

        assert_eq!(names(&root_table), vec!["C", "y"]);

        let index = semantic_index(&db, file);

        let scopes: Vec<_> = index.child_scopes(FileScopeId::root()).collect();
        assert_eq!(scopes.len(), 1);

        let (class_scope_id, class_scope) = scopes[0];
        assert_eq!(class_scope.kind(), ScopeKind::Class);
        assert_eq!(class_scope.name(), "C");

        let class_table = index.symbol_table(class_scope_id);
        assert_eq!(names(&class_table), vec!["x"]);
        assert_eq!(
            class_table.symbol_by_name("x").unwrap().definitions().len(),
            1
        );
    }

    #[test]
    fn function_scope() {
        let TestCase { db, file } = test_case(
            "
def func():
    x = 1
y = 2
",
        );
        let index = semantic_index(&db, file);
        let root_table = index.symbol_table(FileScopeId::root());

        assert_eq!(names(&root_table), vec!["func", "y"]);

        let scopes = index.child_scopes(FileScopeId::root()).collect::<Vec<_>>();
        assert_eq!(scopes.len(), 1);

        let (function_scope_id, function_scope) = scopes[0];
        assert_eq!(function_scope.kind(), ScopeKind::Function);
        assert_eq!(function_scope.name(), "func");

        let function_table = index.symbol_table(function_scope_id);
        assert_eq!(names(&function_table), vec!["x"]);
        assert_eq!(
            function_table
                .symbol_by_name("x")
                .unwrap()
                .definitions()
                .len(),
            1
        );
    }

    #[test]
    fn dupes() {
        let TestCase { db, file } = test_case(
            "
def func():
    x = 1
def func():
    y = 2
",
        );
        let index = semantic_index(&db, file);
        let root_table = index.symbol_table(FileScopeId::root());

        assert_eq!(names(&root_table), vec!["func"]);
        let scopes: Vec<_> = index.child_scopes(FileScopeId::root()).collect();
        assert_eq!(scopes.len(), 2);

        let (func_scope1_id, func_scope_1) = scopes[0];
        let (func_scope2_id, func_scope_2) = scopes[1];

        assert_eq!(func_scope_1.kind(), ScopeKind::Function);
        assert_eq!(func_scope_1.name(), "func");
        assert_eq!(func_scope_2.kind(), ScopeKind::Function);
        assert_eq!(func_scope_2.name(), "func");

        let func1_table = index.symbol_table(func_scope1_id);
        let func2_table = index.symbol_table(func_scope2_id);
        assert_eq!(names(&func1_table), vec!["x"]);
        assert_eq!(names(&func2_table), vec!["y"]);
        assert_eq!(
            root_table
                .symbol_by_name("func")
                .unwrap()
                .definitions()
                .len(),
            2
        );
    }

    #[test]
    fn generic_function() {
        let TestCase { db, file } = test_case(
            "
def func[T]():
    x = 1
",
        );

        let index = semantic_index(&db, file);
        let root_table = index.symbol_table(FileScopeId::root());

        assert_eq!(names(&root_table), vec!["func"]);

        let scopes: Vec<_> = index.child_scopes(FileScopeId::root()).collect();
        assert_eq!(scopes.len(), 1);
        let (ann_scope_id, ann_scope) = scopes[0];

        assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
        assert_eq!(ann_scope.name(), "func");
        let ann_table = index.symbol_table(ann_scope_id);
        assert_eq!(names(&ann_table), vec!["T"]);

        let scopes: Vec<_> = index.child_scopes(ann_scope_id).collect();
        assert_eq!(scopes.len(), 1);
        let (func_scope_id, func_scope) = scopes[0];
        assert_eq!(func_scope.kind(), ScopeKind::Function);
        assert_eq!(func_scope.name(), "func");
        let func_table = index.symbol_table(func_scope_id);
        assert_eq!(names(&func_table), vec!["x"]);
    }

    #[test]
    fn generic_class() {
        let TestCase { db, file } = test_case(
            "
class C[T]:
    x = 1
",
        );

        let index = semantic_index(&db, file);
        let root_table = index.symbol_table(FileScopeId::root());

        assert_eq!(names(&root_table), vec!["C"]);

        let scopes: Vec<_> = index.child_scopes(FileScopeId::root()).collect();

        assert_eq!(scopes.len(), 1);
        let (ann_scope_id, ann_scope) = scopes[0];
        assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
        assert_eq!(ann_scope.name(), "C");
        let ann_table = index.symbol_table(ann_scope_id);
        assert_eq!(names(&ann_table), vec!["T"]);
        assert!(
            ann_table
                .symbol_by_name("T")
                .is_some_and(|s| s.is_defined() && !s.is_used()),
            "type parameters are defined by the scope that introduces them"
        );

        let scopes: Vec<_> = index.child_scopes(ann_scope_id).collect();
        assert_eq!(scopes.len(), 1);
        let (func_scope_id, func_scope) = scopes[0];

        assert_eq!(func_scope.kind(), ScopeKind::Class);
        assert_eq!(func_scope.name(), "C");
        assert_eq!(names(&index.symbol_table(func_scope_id)), vec!["x"]);
    }

    // TODO: After porting the control flow graph.
    // #[test]
    // fn reachability_trivial() {
    //     let parsed = parse("x = 1; x");
    //     let ast = parsed.syntax();
    //     let index = SemanticIndex::from_ast(ast);
    //     let table = &index.symbol_table;
    //     let x_sym = table
    //         .root_symbol_id_by_name("x")
    //         .expect("x symbol should exist");
    //     let ast::Stmt::Expr(ast::StmtExpr { value: x_use, .. }) = &ast.body[1] else {
    //         panic!("should be an expr")
    //     };
    //     let x_defs: Vec<_> = index
    //         .reachable_definitions(x_sym, x_use)
    //         .map(|constrained_definition| constrained_definition.definition)
    //         .collect();
    //     assert_eq!(x_defs.len(), 1);
    //     let Definition::Assignment(node_key) = &x_defs[0] else {
    //         panic!("def should be an assignment")
    //     };
    //     let Some(def_node) = node_key.resolve(ast.into()) else {
    //         panic!("node key should resolve")
    //     };
    //     let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
    //                                      value: ast::Number::Int(num),
    //                                      ..
    //                                  }) = &*def_node.value
    //     else {
    //         panic!("should be a number literal")
    //     };
    //     assert_eq!(*num, 1);
    // }

    #[test]
    fn expression_scope() {
        let TestCase { db, file } = test_case("x = 1;\ndef test():\n  y = 4");

        let index = semantic_index(&db, file);
        let parsed = parsed_module(&db, file);
        let ast = parsed.syntax();

        let x_stmt = ast.body[0].as_assign_stmt().unwrap();
        let x = &x_stmt.targets[0];

        assert_eq!(index.expression_scope(x).kind(), ScopeKind::Module);
        assert_eq!(index.expression_scope_id(x), FileScopeId::root());

        let def = ast.body[1].as_function_def_stmt().unwrap();
        let y_stmt = def.body[0].as_assign_stmt().unwrap();
        let y = &y_stmt.targets[0];

        assert_eq!(index.expression_scope(y).kind(), ScopeKind::Function);
    }

    #[test]
    fn scope_iterators() {
        let TestCase { db, file } = test_case(
            r#"
class Test:
    def foo():
        def bar():
            ...
    def baz():
        pass

def x():
    pass"#,
        );

        let index = semantic_index(&db, file);

        let descendents: Vec<_> = index
            .descendent_scopes(FileScopeId::root())
            .map(|(_, scope)| scope.name().as_str())
            .collect();
        assert_eq!(descendents, vec!["Test", "foo", "bar", "baz", "x"]);

        let children: Vec<_> = index
            .child_scopes(FileScopeId::root())
            .map(|(_, scope)| scope.name.as_str())
            .collect();
        assert_eq!(children, vec!["Test", "x"]);

        let test_class = index.child_scopes(FileScopeId::root()).next().unwrap().0;
        let test_child_scopes: Vec<_> = index
            .child_scopes(test_class)
            .map(|(_, scope)| scope.name.as_str())
            .collect();
        assert_eq!(test_child_scopes, vec!["foo", "baz"]);

        let bar_scope = index
            .descendent_scopes(FileScopeId::root())
            .nth(2)
            .unwrap()
            .0;
        let ancestors: Vec<_> = index
            .ancestor_scopes(bar_scope)
            .map(|(_, scope)| scope.name())
            .collect();

        assert_eq!(ancestors, vec!["bar", "foo", "Test", "<module>"]);
    }
}
