use std::iter::FusedIterator;
use std::sync::Arc;

use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_index::{IndexSlice, IndexVec};

use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::ast_ids::AstIds;
use crate::semantic_index::builder::SemanticIndexBuilder;
use crate::semantic_index::definition::{Definition, DefinitionNodeKey};
use crate::semantic_index::expression::Expression;
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeKey, NodeWithScopeRef, Scope, ScopeId, ScopedSymbolId, SymbolTable,
};
use crate::semantic_index::use_def::UseDefMap;
use crate::Db;

pub mod ast_ids;
mod builder;
pub(crate) mod constraint;
pub mod definition;
pub mod expression;
pub mod symbol;
mod use_def;

pub(crate) use self::use_def::{
    BindingWithConstraints, BindingWithConstraintsIterator, DeclarationsIterator,
};

type SymbolMap = hashbrown::HashMap<ScopedSymbolId, (), ()>;

/// Returns the semantic index for `file`.
///
/// Prefer using [`symbol_table`] when working with symbols from a single scope.
#[salsa::tracked(return_ref, no_eq)]
pub(crate) fn semantic_index(db: &dyn Db, file: File) -> SemanticIndex<'_> {
    let _span = tracing::trace_span!("semantic_index", file = %file.path(db)).entered();

    let parsed = parsed_module(db.upcast(), file);

    SemanticIndexBuilder::new(db, file, parsed).build()
}

/// Returns the symbol table for a specific `scope`.
///
/// Using [`symbol_table`] over [`semantic_index`] has the advantage that
/// Salsa can avoid invalidating dependent queries if this scope's symbol table
/// is unchanged.
#[salsa::tracked]
pub(crate) fn symbol_table<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Arc<SymbolTable> {
    let file = scope.file(db);
    let _span =
        tracing::trace_span!("symbol_table", scope=?scope.as_id(), file=%file.path(db)).entered();
    let index = semantic_index(db, file);

    index.symbol_table(scope.file_scope_id(db))
}

/// Returns the use-def map for a specific `scope`.
///
/// Using [`use_def_map`] over [`semantic_index`] has the advantage that
/// Salsa can avoid invalidating dependent queries if this scope's use-def map
/// is unchanged.
#[salsa::tracked]
pub(crate) fn use_def_map<'db>(db: &'db dyn Db, scope: ScopeId<'db>) -> Arc<UseDefMap<'db>> {
    let file = scope.file(db);
    let _span =
        tracing::trace_span!("use_def_map", scope=?scope.as_id(), file=%file.path(db)).entered();
    let index = semantic_index(db, file);

    index.use_def_map(scope.file_scope_id(db))
}

/// Returns the module global scope of `file`.
#[salsa::tracked]
pub(crate) fn global_scope(db: &dyn Db, file: File) -> ScopeId<'_> {
    let _span = tracing::trace_span!("global_scope", file = %file.path(db)).entered();

    FileScopeId::global().to_scope_id(db, file)
}

/// The symbol tables and use-def maps for all scopes in a file.
#[derive(Debug)]
pub(crate) struct SemanticIndex<'db> {
    /// List of all symbol tables in this file, indexed by scope.
    symbol_tables: IndexVec<FileScopeId, Arc<SymbolTable>>,

    /// List of all scopes in this file.
    scopes: IndexVec<FileScopeId, Scope>,

    /// Map expressions to their corresponding scope.
    scopes_by_expression: FxHashMap<ExpressionNodeKey, FileScopeId>,

    /// Map from a node creating a definition to its definition.
    definitions_by_node: FxHashMap<DefinitionNodeKey, Definition<'db>>,

    /// Map from a standalone expression to its [`Expression`] ingredient.
    expressions_by_node: FxHashMap<ExpressionNodeKey, Expression<'db>>,

    /// Map from nodes that create a scope to the scope they create.
    scopes_by_node: FxHashMap<NodeWithScopeKey, FileScopeId>,

    /// Map from the file-local [`FileScopeId`] to the salsa-ingredient [`ScopeId`].
    scope_ids_by_scope: IndexVec<FileScopeId, ScopeId<'db>>,

    /// Use-def map for each scope in this file.
    use_def_maps: IndexVec<FileScopeId, Arc<UseDefMap<'db>>>,

    /// Lookup table to map between node ids and ast nodes.
    ///
    /// Note: We should not depend on this map when analysing other files or
    /// changing a file invalidates all dependents.
    ast_ids: IndexVec<FileScopeId, AstIds>,

    /// Flags about the global scope (code usage impacting inference)
    has_future_annotations: bool,
}

impl<'db> SemanticIndex<'db> {
    /// Returns the symbol table for a specific scope.
    ///
    /// Use the Salsa cached [`symbol_table()`] query if you only need the
    /// symbol table for a single scope.
    pub(super) fn symbol_table(&self, scope_id: FileScopeId) -> Arc<SymbolTable> {
        self.symbol_tables[scope_id].clone()
    }

    /// Returns the use-def map for a specific scope.
    ///
    /// Use the Salsa cached [`use_def_map()`] query if you only need the
    /// use-def map for a single scope.
    pub(super) fn use_def_map(&self, scope_id: FileScopeId) -> Arc<UseDefMap> {
        self.use_def_maps[scope_id].clone()
    }

    pub(crate) fn ast_ids(&self, scope_id: FileScopeId) -> &AstIds {
        &self.ast_ids[scope_id]
    }

    /// Returns the ID of the `expression`'s enclosing scope.
    pub(crate) fn expression_scope_id(
        &self,
        expression: impl Into<ExpressionNodeKey>,
    ) -> FileScopeId {
        self.scopes_by_expression[&expression.into()]
    }

    /// Returns the [`Scope`] of the `expression`'s enclosing scope.
    #[allow(unused)]
    pub(crate) fn expression_scope(&self, expression: impl Into<ExpressionNodeKey>) -> &Scope {
        &self.scopes[self.expression_scope_id(expression)]
    }

    /// Returns the [`Scope`] with the given id.
    pub(crate) fn scope(&self, id: FileScopeId) -> &Scope {
        &self.scopes[id]
    }

    pub(crate) fn scope_ids(&self) -> impl Iterator<Item = ScopeId> {
        self.scope_ids_by_scope.iter().copied()
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
    #[allow(unused)]
    pub(crate) fn child_scopes(&self, scope: FileScopeId) -> ChildrenIter {
        ChildrenIter::new(self, scope)
    }

    /// Returns an iterator over all ancestors of `scope`, starting with `scope` itself.
    #[allow(unused)]
    pub(crate) fn ancestor_scopes(&self, scope: FileScopeId) -> AncestorsIter {
        AncestorsIter::new(self, scope)
    }

    /// Returns the [`Definition`] salsa ingredient for `definition_key`.
    pub(crate) fn definition(
        &self,
        definition_key: impl Into<DefinitionNodeKey>,
    ) -> Definition<'db> {
        self.definitions_by_node[&definition_key.into()]
    }

    /// Returns the [`Expression`] ingredient for an expression node.
    /// Panics if we have no expression ingredient for that node. We can only call this method for
    /// standalone-inferable expressions, which we call `add_standalone_expression` for in
    /// [`SemanticIndexBuilder`].
    pub(crate) fn expression(
        &self,
        expression_key: impl Into<ExpressionNodeKey>,
    ) -> Expression<'db> {
        self.expressions_by_node[&expression_key.into()]
    }

    /// Returns the id of the scope that `node` creates. This is different from [`Definition::scope`] which
    /// returns the scope in which that definition is defined in.
    pub(crate) fn node_scope(&self, node: NodeWithScopeRef) -> FileScopeId {
        self.scopes_by_node[&node.node_key()]
    }

    /// Checks if there is an import of `__future__.annotations` in the global scope, which affects
    /// the logic for type inference.
    pub(super) fn has_future_annotations(&self) -> bool {
        self.has_future_annotations
    }
}

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

#[cfg(test)]
mod tests {
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithTestSystem;
    use ruff_python_ast as ast;
    use ruff_text_size::{Ranged, TextRange};

    use crate::db::tests::TestDb;
    use crate::semantic_index::ast_ids::{HasScopedUseId, ScopedUseId};
    use crate::semantic_index::definition::{Definition, DefinitionKind};
    use crate::semantic_index::symbol::{
        FileScopeId, Scope, ScopeKind, ScopedSymbolId, SymbolTable,
    };
    use crate::semantic_index::use_def::UseDefMap;
    use crate::semantic_index::{global_scope, semantic_index, symbol_table, use_def_map};
    use crate::Db;

    impl UseDefMap<'_> {
        fn first_public_binding(&self, symbol: ScopedSymbolId) -> Option<Definition<'_>> {
            self.public_bindings(symbol)
                .next()
                .map(|constrained_binding| constrained_binding.binding)
        }

        fn first_binding_at_use(&self, use_id: ScopedUseId) -> Option<Definition<'_>> {
            self.bindings_at_use(use_id)
                .next()
                .map(|constrained_binding| constrained_binding.binding)
        }
    }

    struct TestCase {
        db: TestDb,
        file: File,
    }

    fn test_case(content: impl ToString) -> TestCase {
        let mut db = TestDb::new();
        db.write_file("test.py", content).unwrap();

        let file = system_path_to_file(&db, "test.py").unwrap();

        TestCase { db, file }
    }

    fn names(table: &SymbolTable) -> Vec<String> {
        table
            .symbols()
            .map(|symbol| symbol.name().to_string())
            .collect()
    }

    #[test]
    fn empty() {
        let TestCase { db, file } = test_case("");
        let global_table = symbol_table(&db, global_scope(&db, file));

        let global_names = names(&global_table);

        assert_eq!(global_names, Vec::<&str>::new());
    }

    #[test]
    fn simple() {
        let TestCase { db, file } = test_case("x");
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert_eq!(names(&global_table), vec!["x"]);
    }

    #[test]
    fn annotation_only() {
        let TestCase { db, file } = test_case("x: int");
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert_eq!(names(&global_table), vec!["int", "x"]);
        // TODO record definition
    }

    #[test]
    fn import() {
        let TestCase { db, file } = test_case("import foo");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(names(&global_table), vec!["foo"]);
        let foo = global_table.symbol_id_by_name("foo").unwrap();

        let use_def = use_def_map(&db, scope);
        let binding = use_def.first_public_binding(foo).unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Import(_)));
    }

    #[test]
    fn import_sub() {
        let TestCase { db, file } = test_case("import foo.bar");
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert_eq!(names(&global_table), vec!["foo"]);
    }

    #[test]
    fn import_as() {
        let TestCase { db, file } = test_case("import foo.bar as baz");
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert_eq!(names(&global_table), vec!["baz"]);
    }

    #[test]
    fn import_from() {
        let TestCase { db, file } = test_case("from bar import foo");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(names(&global_table), vec!["foo"]);
        assert!(
            global_table
                .symbol_by_name("foo")
                .is_some_and(|symbol| { symbol.is_bound() && !symbol.is_used() }),
            "symbols that are defined get the defined flag"
        );

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(
                global_table
                    .symbol_id_by_name("foo")
                    .expect("symbol to exist"),
            )
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::ImportFrom(_)));
    }

    #[test]
    fn assign() {
        let TestCase { db, file } = test_case("x = foo");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(names(&global_table), vec!["foo", "x"]);
        assert!(
            global_table
                .symbol_by_name("foo")
                .is_some_and(|symbol| { !symbol.is_bound() && symbol.is_used() }),
            "a symbol used but not bound in a scope should have only the used flag"
        );
        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id_by_name("x").expect("symbol exists"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Assignment(_)));
    }

    #[test]
    fn augmented_assignment() {
        let TestCase { db, file } = test_case("x += 1");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(names(&global_table), vec!["x"]);

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id_by_name("x").unwrap())
            .unwrap();

        assert!(matches!(
            binding.kind(&db),
            DefinitionKind::AugmentedAssignment(_)
        ));
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
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert_eq!(names(&global_table), vec!["C", "y"]);

        let index = semantic_index(&db, file);

        let [(class_scope_id, class_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };
        assert_eq!(class_scope.kind(), ScopeKind::Class);
        assert_eq!(class_scope_id.to_scope_id(&db, file).name(&db), "C");

        let class_table = index.symbol_table(class_scope_id);
        assert_eq!(names(&class_table), vec!["x"]);

        let use_def = index.use_def_map(class_scope_id);
        let binding = use_def
            .first_public_binding(class_table.symbol_id_by_name("x").expect("symbol exists"))
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Assignment(_)));
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
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["func", "y"]);

        let [(function_scope_id, function_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };
        assert_eq!(function_scope.kind(), ScopeKind::Function);
        assert_eq!(function_scope_id.to_scope_id(&db, file).name(&db), "func");

        let function_table = index.symbol_table(function_scope_id);
        assert_eq!(names(&function_table), vec!["x"]);

        let use_def = index.use_def_map(function_scope_id);
        let binding = use_def
            .first_public_binding(
                function_table
                    .symbol_id_by_name("x")
                    .expect("symbol exists"),
            )
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Assignment(_)));
    }

    #[test]
    fn function_parameter_symbols() {
        let TestCase { db, file } = test_case(
            "
def f(a: str, /, b: str, c: int = 1, *args, d: int = 2, **kwargs):
    pass
",
        );

        let index = semantic_index(&db, file);
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert_eq!(names(&global_table), vec!["str", "int", "f"]);

        let [(function_scope_id, _function_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("Expected a function scope")
        };

        let function_table = index.symbol_table(function_scope_id);
        assert_eq!(
            names(&function_table),
            vec!["a", "b", "c", "args", "d", "kwargs"],
        );

        let use_def = index.use_def_map(function_scope_id);
        for name in ["a", "b", "c", "d"] {
            let binding = use_def
                .first_public_binding(
                    function_table
                        .symbol_id_by_name(name)
                        .expect("symbol exists"),
                )
                .unwrap();
            assert!(matches!(
                binding.kind(&db),
                DefinitionKind::ParameterWithDefault(_)
            ));
        }
        for name in ["args", "kwargs"] {
            let binding = use_def
                .first_public_binding(
                    function_table
                        .symbol_id_by_name(name)
                        .expect("symbol exists"),
                )
                .unwrap();
            assert!(matches!(binding.kind(&db), DefinitionKind::Parameter(_)));
        }
    }

    #[test]
    fn lambda_parameter_symbols() {
        let TestCase { db, file } = test_case("lambda a, b, c=1, *args, d=2, **kwargs: None");

        let index = semantic_index(&db, file);
        let global_table = symbol_table(&db, global_scope(&db, file));

        assert!(names(&global_table).is_empty());

        let [(lambda_scope_id, _lambda_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("Expected a lambda scope")
        };

        let lambda_table = index.symbol_table(lambda_scope_id);
        assert_eq!(
            names(&lambda_table),
            vec!["a", "b", "c", "args", "d", "kwargs"],
        );

        let use_def = index.use_def_map(lambda_scope_id);
        for name in ["a", "b", "c", "d"] {
            let binding = use_def
                .first_public_binding(lambda_table.symbol_id_by_name(name).expect("symbol exists"))
                .unwrap();
            assert!(matches!(
                binding.kind(&db),
                DefinitionKind::ParameterWithDefault(_)
            ));
        }
        for name in ["args", "kwargs"] {
            let binding = use_def
                .first_public_binding(lambda_table.symbol_id_by_name(name).expect("symbol exists"))
                .unwrap();
            assert!(matches!(binding.kind(&db), DefinitionKind::Parameter(_)));
        }
    }

    /// Test case to validate that the comprehension scope is correctly identified and that the target
    /// variable is defined only in the comprehension scope and not in the global scope.
    #[test]
    fn comprehension_scope() {
        let TestCase { db, file } = test_case(
            "
[x for x, y in iter1]
",
        );

        let index = semantic_index(&db, file);
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["iter1"]);

        let [(comprehension_scope_id, comprehension_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };

        assert_eq!(comprehension_scope.kind(), ScopeKind::Comprehension);
        assert_eq!(
            comprehension_scope_id.to_scope_id(&db, file).name(&db),
            "<listcomp>"
        );

        let comprehension_symbol_table = index.symbol_table(comprehension_scope_id);

        assert_eq!(names(&comprehension_symbol_table), vec!["x", "y"]);

        let use_def = index.use_def_map(comprehension_scope_id);
        for name in ["x", "y"] {
            let binding = use_def
                .first_public_binding(
                    comprehension_symbol_table
                        .symbol_id_by_name(name)
                        .expect("symbol exists"),
                )
                .unwrap();
            assert!(matches!(
                binding.kind(&db),
                DefinitionKind::Comprehension(_)
            ));
        }
    }

    /// Test case to validate that the `x` variable used in the comprehension is referencing the
    /// `x` variable defined by the inner generator (`for x in iter2`) and not the outer one.
    #[test]
    fn multiple_generators() {
        let TestCase { db, file } = test_case(
            "
[x for x in iter1 for x in iter2]
",
        );

        let index = semantic_index(&db, file);
        let [(comprehension_scope_id, _)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };

        let use_def = index.use_def_map(comprehension_scope_id);

        let module = parsed_module(&db, file).syntax();
        let element = module.body[0]
            .as_expr_stmt()
            .unwrap()
            .value
            .as_list_comp_expr()
            .unwrap()
            .elt
            .as_name_expr()
            .unwrap();
        let element_use_id =
            element.scoped_use_id(&db, comprehension_scope_id.to_scope_id(&db, file));

        let binding = use_def.first_binding_at_use(element_use_id).unwrap();
        let DefinitionKind::Comprehension(comprehension) = binding.kind(&db) else {
            panic!("expected generator definition")
        };
        let target = comprehension.target();
        let name = target.id().as_str();

        assert_eq!(name, "x");
        assert_eq!(target.range(), TextRange::new(23.into(), 24.into()));
    }

    /// Test case to validate that the nested comprehension creates a new scope which is a child of
    /// the outer comprehension scope and the variables are correctly defined in the respective
    /// scopes.
    #[test]
    fn nested_generators() {
        let TestCase { db, file } = test_case(
            "
[{x for x in iter2} for y in iter1]
",
        );

        let index = semantic_index(&db, file);
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["iter1"]);

        let [(comprehension_scope_id, comprehension_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope")
        };

        assert_eq!(comprehension_scope.kind(), ScopeKind::Comprehension);
        assert_eq!(
            comprehension_scope_id.to_scope_id(&db, file).name(&db),
            "<listcomp>"
        );

        let comprehension_symbol_table = index.symbol_table(comprehension_scope_id);

        assert_eq!(names(&comprehension_symbol_table), vec!["y", "iter2"]);

        let [(inner_comprehension_scope_id, inner_comprehension_scope)] = index
            .child_scopes(comprehension_scope_id)
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one inner generator scope")
        };

        assert_eq!(inner_comprehension_scope.kind(), ScopeKind::Comprehension);
        assert_eq!(
            inner_comprehension_scope_id
                .to_scope_id(&db, file)
                .name(&db),
            "<setcomp>"
        );

        let inner_comprehension_symbol_table = index.symbol_table(inner_comprehension_scope_id);

        assert_eq!(names(&inner_comprehension_symbol_table), vec!["x"]);
    }

    #[test]
    fn with_item_definition() {
        let TestCase { db, file } = test_case(
            "
with item1 as x, item2 as y:
    pass
",
        );

        let index = semantic_index(&db, file);
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["item1", "x", "item2", "y"]);

        let use_def = index.use_def_map(FileScopeId::global());
        for name in ["x", "y"] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id_by_name(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            assert!(matches!(binding.kind(&db), DefinitionKind::WithItem(_)));
        }
    }

    #[test]
    fn with_item_unpacked_definition() {
        let TestCase { db, file } = test_case(
            "
with context() as (x, y):
    pass
",
        );

        let index = semantic_index(&db, file);
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["context", "x", "y"]);

        let use_def = index.use_def_map(FileScopeId::global());
        for name in ["x", "y"] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id_by_name(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            assert!(matches!(binding.kind(&db), DefinitionKind::WithItem(_)));
        }
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
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["func"]);
        let [(func_scope1_id, func_scope_1), (func_scope2_id, func_scope_2)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected two child scopes");
        };

        assert_eq!(func_scope_1.kind(), ScopeKind::Function);

        assert_eq!(func_scope1_id.to_scope_id(&db, file).name(&db), "func");
        assert_eq!(func_scope_2.kind(), ScopeKind::Function);
        assert_eq!(func_scope2_id.to_scope_id(&db, file).name(&db), "func");

        let func1_table = index.symbol_table(func_scope1_id);
        let func2_table = index.symbol_table(func_scope2_id);
        assert_eq!(names(&func1_table), vec!["x"]);
        assert_eq!(names(&func2_table), vec!["y"]);

        let use_def = index.use_def_map(FileScopeId::global());
        let binding = use_def
            .first_public_binding(
                global_table
                    .symbol_id_by_name("func")
                    .expect("symbol exists"),
            )
            .unwrap();
        assert!(matches!(binding.kind(&db), DefinitionKind::Function(_)));
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
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["func"]);

        let [(ann_scope_id, ann_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };

        assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
        assert_eq!(ann_scope_id.to_scope_id(&db, file).name(&db), "func");
        let ann_table = index.symbol_table(ann_scope_id);
        assert_eq!(names(&ann_table), vec!["T"]);

        let [(func_scope_id, func_scope)] =
            index.child_scopes(ann_scope_id).collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };
        assert_eq!(func_scope.kind(), ScopeKind::Function);
        assert_eq!(func_scope_id.to_scope_id(&db, file).name(&db), "func");
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
        let global_table = index.symbol_table(FileScopeId::global());

        assert_eq!(names(&global_table), vec!["C"]);

        let [(ann_scope_id, ann_scope)] = index
            .child_scopes(FileScopeId::global())
            .collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };

        assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
        assert_eq!(ann_scope_id.to_scope_id(&db, file).name(&db), "C");
        let ann_table = index.symbol_table(ann_scope_id);
        assert_eq!(names(&ann_table), vec!["T"]);
        assert!(
            ann_table
                .symbol_by_name("T")
                .is_some_and(|s| s.is_bound() && !s.is_used()),
            "type parameters are defined by the scope that introduces them"
        );

        let [(class_scope_id, class_scope)] =
            index.child_scopes(ann_scope_id).collect::<Vec<_>>()[..]
        else {
            panic!("expected one child scope");
        };

        assert_eq!(class_scope.kind(), ScopeKind::Class);
        assert_eq!(class_scope_id.to_scope_id(&db, file).name(&db), "C");
        assert_eq!(names(&index.symbol_table(class_scope_id)), vec!["x"]);
    }

    #[test]
    fn reachability_trivial() {
        let TestCase { db, file } = test_case("x = 1; x");
        let parsed = parsed_module(&db, file);
        let scope = global_scope(&db, file);
        let ast = parsed.syntax();
        let ast::Stmt::Expr(ast::StmtExpr {
            value: x_use_expr, ..
        }) = &ast.body[1]
        else {
            panic!("should be an expr")
        };
        let ast::Expr::Name(x_use_expr_name) = x_use_expr.as_ref() else {
            panic!("expected a Name");
        };
        let x_use_id = x_use_expr_name.scoped_use_id(&db, scope);
        let use_def = use_def_map(&db, scope);
        let binding = use_def.first_binding_at_use(x_use_id).unwrap();
        let DefinitionKind::Assignment(assignment) = binding.kind(&db) else {
            panic!("should be an assignment definition")
        };
        let ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(num),
            ..
        }) = &*assignment.assignment().value
        else {
            panic!("should be a number literal")
        };
        assert_eq!(*num, 1);
    }

    #[test]
    fn expression_scope() {
        let TestCase { db, file } = test_case("x = 1;\ndef test():\n  y = 4");

        let index = semantic_index(&db, file);
        let parsed = parsed_module(&db, file);
        let ast = parsed.syntax();

        let x_stmt = ast.body[0].as_assign_stmt().unwrap();
        let x = &x_stmt.targets[0];

        assert_eq!(index.expression_scope(x).kind(), ScopeKind::Module);
        assert_eq!(index.expression_scope_id(x), FileScopeId::global());

        let def = ast.body[1].as_function_def_stmt().unwrap();
        let y_stmt = def.body[0].as_assign_stmt().unwrap();
        let y = &y_stmt.targets[0];

        assert_eq!(index.expression_scope(y).kind(), ScopeKind::Function);
    }

    #[test]
    fn scope_iterators() {
        fn scope_names<'a>(
            scopes: impl Iterator<Item = (FileScopeId, &'a Scope)>,
            db: &'a dyn Db,
            file: File,
        ) -> Vec<&'a str> {
            scopes
                .into_iter()
                .map(|(scope_id, _)| scope_id.to_scope_id(db, file).name(db))
                .collect()
        }

        let TestCase { db, file } = test_case(
            r"
class Test:
    def foo():
        def bar():
            ...
    def baz():
        pass

def x():
    pass",
        );

        let index = semantic_index(&db, file);

        let descendents = index.descendent_scopes(FileScopeId::global());
        assert_eq!(
            scope_names(descendents, &db, file),
            vec!["Test", "foo", "bar", "baz", "x"]
        );

        let children = index.child_scopes(FileScopeId::global());
        assert_eq!(scope_names(children, &db, file), vec!["Test", "x"]);

        let test_class = index.child_scopes(FileScopeId::global()).next().unwrap().0;
        let test_child_scopes = index.child_scopes(test_class);
        assert_eq!(
            scope_names(test_child_scopes, &db, file),
            vec!["foo", "baz"]
        );

        let bar_scope = index
            .descendent_scopes(FileScopeId::global())
            .nth(2)
            .unwrap()
            .0;
        let ancestors = index.ancestor_scopes(bar_scope);

        assert_eq!(
            scope_names(ancestors, &db, file),
            vec!["bar", "foo", "Test", "<module>"]
        );
    }

    #[test]
    fn match_stmt() {
        let TestCase { db, file } = test_case(
            "
match subject:
    case a: ...
    case [b, c, *d]: ...
    case e as f: ...
    case {'x': g, **h}: ...
    case Foo(i, z=j): ...
    case k | l: ...
    case _: ...
",
        );

        let global_scope_id = global_scope(&db, file);
        let global_table = symbol_table(&db, global_scope_id);

        assert!(global_table.symbol_by_name("Foo").unwrap().is_used());
        assert_eq!(
            names(&global_table),
            vec!["subject", "a", "b", "c", "d", "e", "f", "g", "h", "Foo", "i", "j", "k", "l"]
        );

        let use_def = use_def_map(&db, global_scope_id);
        for (name, expected_index) in [
            ("a", 0),
            ("b", 0),
            ("c", 1),
            ("d", 2),
            ("e", 0),
            ("f", 1),
            ("g", 0),
            ("h", 1),
            ("i", 0),
            ("j", 1),
            ("k", 0),
            ("l", 1),
        ] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id_by_name(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            if let DefinitionKind::MatchPattern(pattern) = binding.kind(&db) {
                assert_eq!(pattern.index(), expected_index);
            } else {
                panic!("Expected match pattern definition for {name}");
            }
        }
    }

    #[test]
    fn nested_match_case() {
        let TestCase { db, file } = test_case(
            "
match 1:
    case first:
        match 2:
            case second:
                pass
",
        );

        let global_scope_id = global_scope(&db, file);
        let global_table = symbol_table(&db, global_scope_id);

        assert_eq!(names(&global_table), vec!["first", "second"]);

        let use_def = use_def_map(&db, global_scope_id);
        for (name, expected_index) in [("first", 0), ("second", 0)] {
            let binding = use_def
                .first_public_binding(global_table.symbol_id_by_name(name).expect("symbol exists"))
                .expect("Expected with item definition for {name}");
            if let DefinitionKind::MatchPattern(pattern) = binding.kind(&db) {
                assert_eq!(pattern.index(), expected_index);
            } else {
                panic!("Expected match pattern definition for {name}");
            }
        }
    }

    #[test]
    fn for_loops_single_assignment() {
        let TestCase { db, file } = test_case("for x in a: pass");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(&names(&global_table), &["a", "x"]);

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id_by_name("x").unwrap())
            .unwrap();

        assert!(matches!(binding.kind(&db), DefinitionKind::For(_)));
    }

    #[test]
    fn for_loops_simple_unpacking() {
        let TestCase { db, file } = test_case("for (x, y) in a: pass");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(&names(&global_table), &["a", "x", "y"]);

        let use_def = use_def_map(&db, scope);
        let x_binding = use_def
            .first_public_binding(global_table.symbol_id_by_name("x").unwrap())
            .unwrap();
        let y_binding = use_def
            .first_public_binding(global_table.symbol_id_by_name("y").unwrap())
            .unwrap();

        assert!(matches!(x_binding.kind(&db), DefinitionKind::For(_)));
        assert!(matches!(y_binding.kind(&db), DefinitionKind::For(_)));
    }

    #[test]
    fn for_loops_complex_unpacking() {
        let TestCase { db, file } = test_case("for [((a,) b), (c, d)] in e: pass");
        let scope = global_scope(&db, file);
        let global_table = symbol_table(&db, scope);

        assert_eq!(&names(&global_table), &["e", "a", "b", "c", "d"]);

        let use_def = use_def_map(&db, scope);
        let binding = use_def
            .first_public_binding(global_table.symbol_id_by_name("a").unwrap())
            .unwrap();

        assert!(matches!(binding.kind(&db), DefinitionKind::For(_)));
    }
}
