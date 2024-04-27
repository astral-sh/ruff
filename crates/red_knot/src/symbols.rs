#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::iter::{Copied, DoubleEndedIterator, FusedIterator};
use std::num::NonZeroU32;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use hashbrown::hash_map::{Keys, RawEntryMut};
use rustc_hash::{FxHashMap, FxHasher};

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::visitor::preorder::PreorderVisitor;

use crate::ast_ids::TypedNodeKey;
use crate::cache::KeyValueCache;
use crate::db::{HasJar, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::module::ModuleName;
use crate::Name;

#[allow(unreachable_pub)]
#[tracing::instrument(level = "debug", skip(db))]
pub fn symbol_table<Db>(db: &Db, file_id: FileId) -> Arc<SymbolTable>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let jar = db.jar();

    jar.symbol_tables.get(&file_id, |_| {
        let parsed = db.parse(file_id);
        Arc::from(SymbolTable::from_ast(parsed.ast()))
    })
}

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

#[newtype_index]
pub(crate) struct ScopeId;

impl ScopeId {
    pub(crate) fn scope(self, table: &SymbolTable) -> &Scope {
        &table.scopes_by_id[self]
    }
}

#[newtype_index]
pub struct SymbolId;

impl SymbolId {
    pub(crate) fn symbol(self, table: &SymbolTable) -> &Symbol {
        &table.symbols_by_id[self]
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

#[derive(Debug)]
pub(crate) struct Scope {
    name: Name,
    kind: ScopeKind,
    child_scopes: Vec<ScopeId>,
    // symbol IDs, hashed by symbol name
    symbols_by_name: Map<SymbolId, ()>,
}

impl Scope {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    pub(crate) fn kind(&self) -> ScopeKind {
        self.kind
    }
}

#[derive(Debug)]
pub(crate) struct Symbol {
    name: Name,
}

impl Symbol {
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }
}

// TODO storing TypedNodeKey for definitions means we have to search to find them again in the AST;
// this is at best O(log n). If looking up definitions is a bottleneck we should look for
// alternatives here.
#[derive(Debug)]
pub(crate) enum Definition {
    // For the import cases, we don't need reference to any arbitrary AST subtrees (annotations,
    // RHS), and referencing just the import statement node is imprecise (a single import statement
    // can assign many symbols, we'd have to re-search for the one we care about), so we just copy
    // the small amount of information we need from the AST.
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(TypedNodeKey<ast::StmtClassDef>),
    FunctionDef(TypedNodeKey<ast::StmtFunctionDef>),
    Assignment(TypedNodeKey<ast::StmtAssign>),
    AnnotatedAssignment(TypedNodeKey<ast::StmtAnnAssign>),
    // TODO with statements, except handlers, function args...
}

#[derive(Debug)]
pub(crate) struct ImportDefinition {
    pub(crate) module: ModuleName,
}

#[derive(Debug)]
pub(crate) struct ImportFromDefinition {
    pub(crate) module: Option<ModuleName>,
    pub(crate) name: Name,
    pub(crate) level: u32,
}

#[derive(Debug, Clone)]
pub enum Dependency {
    Module(ModuleName),
    Relative {
        level: NonZeroU32,
        module: Option<ModuleName>,
    },
}

/// Table of all symbols in all scopes for a module.
#[derive(Debug)]
pub struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
    defs: FxHashMap<SymbolId, Vec<Definition>>,
    dependencies: Vec<Dependency>,
}

impl SymbolTable {
    pub(crate) fn from_ast(module: &ast::ModModule) -> Self {
        let root_scope_id = SymbolTable::root_scope_id();
        let mut builder = SymbolTableBuilder {
            table: SymbolTable::new(),
            scopes: vec![root_scope_id],
        };
        builder.visit_body(&module.body);
        builder.table
    }

    pub(crate) fn new() -> Self {
        let mut table = SymbolTable {
            scopes_by_id: IndexVec::new(),
            symbols_by_id: IndexVec::new(),
            defs: FxHashMap::default(),
            dependencies: Vec::new(),
        };
        table.scopes_by_id.push(Scope {
            name: Name::new("<module>"),
            kind: ScopeKind::Module,
            child_scopes: Vec::new(),
            symbols_by_name: Map::default(),
        });
        table
    }

    pub(crate) fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    pub(crate) const fn root_scope_id() -> ScopeId {
        ScopeId::from_usize(0)
    }

    pub(crate) fn root_scope(&self) -> &Scope {
        &self.scopes_by_id[SymbolTable::root_scope_id()]
    }

    pub(crate) fn symbol_ids_for_scope(&self, scope_id: ScopeId) -> Copied<Keys<SymbolId, ()>> {
        self.scopes_by_id[scope_id].symbols_by_name.keys().copied()
    }

    pub(crate) fn symbols_for_scope(
        &self,
        scope_id: ScopeId,
    ) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        SymbolIterator {
            table: self,
            ids: self.symbol_ids_for_scope(scope_id),
        }
    }

    pub(crate) fn root_symbol_ids(&self) -> Copied<Keys<SymbolId, ()>> {
        self.symbol_ids_for_scope(SymbolTable::root_scope_id())
    }

    pub(crate) fn root_symbols(&self) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        self.symbols_for_scope(SymbolTable::root_scope_id())
    }

    pub(crate) fn child_scope_ids_of(&self, scope_id: ScopeId) -> &[ScopeId] {
        &self.scopes_by_id[scope_id].child_scopes
    }

    pub(crate) fn child_scopes_of(&self, scope_id: ScopeId) -> ScopeIterator<&[ScopeId]> {
        ScopeIterator {
            table: self,
            ids: self.child_scope_ids_of(scope_id),
        }
    }

    pub(crate) fn root_child_scope_ids(&self) -> &[ScopeId] {
        self.child_scope_ids_of(SymbolTable::root_scope_id())
    }

    pub(crate) fn root_child_scopes(&self) -> ScopeIterator<&[ScopeId]> {
        self.child_scopes_of(SymbolTable::root_scope_id())
    }

    pub(crate) fn symbol_id_by_name(&self, scope_id: ScopeId, name: &str) -> Option<SymbolId> {
        let scope = &self.scopes_by_id[scope_id];
        let hash = SymbolTable::hash_name(name);
        let name = Name::new(name);
        scope
            .symbols_by_name
            .raw_entry()
            .from_hash(hash, |symid| self.symbols_by_id[*symid].name == name)
            .map(|(symbol_id, ())| *symbol_id)
    }

    pub(crate) fn symbol_by_name(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        Some(&self.symbols_by_id[self.symbol_id_by_name(scope_id, name)?])
    }

    pub(crate) fn root_symbol_id_by_name(&self, name: &str) -> Option<SymbolId> {
        self.symbol_id_by_name(SymbolTable::root_scope_id(), name)
    }

    pub(crate) fn root_symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        self.symbol_by_name(SymbolTable::root_scope_id(), name)
    }

    pub(crate) fn defs(&self, symbol_id: SymbolId) -> &[Definition] {
        self.defs
            .get(&symbol_id)
            .map(std::vec::Vec::as_slice)
            .unwrap_or_default()
    }

    fn add_symbol_to_scope(&mut self, scope_id: ScopeId, name: &str) -> SymbolId {
        let hash = SymbolTable::hash_name(name);
        let scope = &mut self.scopes_by_id[scope_id];
        let name = Name::new(name);

        let entry = scope
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |existing| self.symbols_by_id[*existing].name == name);

        match entry {
            RawEntryMut::Occupied(entry) => *entry.key(),
            RawEntryMut::Vacant(entry) => {
                let id = self.symbols_by_id.push(Symbol { name });
                entry.insert_with_hasher(hash, id, (), |_| hash);
                id
            }
        }
    }

    fn add_child_scope(
        &mut self,
        parent_scope_id: ScopeId,
        name: &str,
        kind: ScopeKind,
    ) -> ScopeId {
        let new_scope_id = self.scopes_by_id.push(Scope {
            name: Name::new(name),
            kind,
            child_scopes: Vec::new(),
            symbols_by_name: Map::default(),
        });
        let parent_scope = &mut self.scopes_by_id[parent_scope_id];
        parent_scope.child_scopes.push(new_scope_id);
        new_scope_id
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

pub(crate) struct SymbolIterator<'a, I> {
    table: &'a SymbolTable,
    ids: I,
}

impl<'a, I> Iterator for SymbolIterator<'a, I>
where
    I: Iterator<Item = SymbolId>,
{
    type Item = &'a Symbol;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.next()?;
        Some(&self.table.symbols_by_id[id])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ids.size_hint()
    }
}

impl<'a, I> FusedIterator for SymbolIterator<'a, I> where
    I: Iterator<Item = SymbolId> + FusedIterator
{
}

impl<'a, I> DoubleEndedIterator for SymbolIterator<'a, I>
where
    I: Iterator<Item = SymbolId> + DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let id = self.ids.next_back()?;
        Some(&self.table.symbols_by_id[id])
    }
}

pub(crate) struct ScopeIterator<'a, I> {
    table: &'a SymbolTable,
    ids: I,
}

impl<'a, I> Iterator for ScopeIterator<'a, I>
where
    I: Iterator<Item = ScopeId>,
{
    type Item = &'a Scope;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.next()?;
        Some(&self.table.scopes_by_id[id])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ids.size_hint()
    }
}

impl<'a, I> FusedIterator for ScopeIterator<'a, I> where I: Iterator<Item = ScopeId> + FusedIterator {}

impl<'a, I> DoubleEndedIterator for ScopeIterator<'a, I>
where
    I: Iterator<Item = ScopeId> + DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let id = self.ids.next_back()?;
        Some(&self.table.scopes_by_id[id])
    }
}

struct SymbolTableBuilder {
    table: SymbolTable,
    scopes: Vec<ScopeId>,
}

impl SymbolTableBuilder {
    fn add_symbol(&mut self, identifier: &str) -> SymbolId {
        self.table.add_symbol_to_scope(self.cur_scope(), identifier)
    }

    fn add_symbol_with_def(&mut self, identifier: &str, definition: Definition) -> SymbolId {
        let symbol_id = self.add_symbol(identifier);
        self.table
            .defs
            .entry(symbol_id)
            .or_default()
            .push(definition);
        symbol_id
    }

    fn push_scope(&mut self, child_of: ScopeId, name: &str, kind: ScopeKind) -> ScopeId {
        let scope_id = self.table.add_child_scope(child_of, name, kind);
        self.scopes.push(scope_id);
        scope_id
    }

    fn pop_scope(&mut self) -> ScopeId {
        self.scopes
            .pop()
            .expect("Scope stack should never be empty")
    }

    fn cur_scope(&self) -> ScopeId {
        *self
            .scopes
            .last()
            .expect("Scope stack should never be empty")
    }

    fn with_type_params(
        &mut self,
        name: &str,
        params: &Option<Box<ast::TypeParams>>,
        nested: impl FnOnce(&mut Self),
    ) {
        if let Some(type_params) = params {
            self.push_scope(self.cur_scope(), name, ScopeKind::Annotation);
            for type_param in &type_params.type_params {
                let name = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) => name,
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, .. }) => name,
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, .. }) => name,
                };
                self.add_symbol(name);
            }
        }
        nested(self);
        if params.is_some() {
            self.pop_scope();
        }
    }
}

impl PreorderVisitor<'_> for SymbolTableBuilder {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = expr {
            self.add_symbol(id);
        }
        ast::visitor::preorder::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        // TODO need to capture more definition statements here
        match stmt {
            ast::Stmt::ClassDef(node) => {
                let def = Definition::ClassDef(TypedNodeKey::from_node(node));
                self.add_symbol_with_def(&node.name, def);
                self.with_type_params(&node.name, &node.type_params, |builder| {
                    builder.push_scope(builder.cur_scope(), &node.name, ScopeKind::Class);
                    ast::visitor::preorder::walk_stmt(builder, stmt);
                    builder.pop_scope();
                });
            }
            ast::Stmt::FunctionDef(node) => {
                let def = Definition::FunctionDef(TypedNodeKey::from_node(node));
                self.add_symbol_with_def(&node.name, def);
                self.with_type_params(&node.name, &node.type_params, |builder| {
                    builder.push_scope(builder.cur_scope(), &node.name, ScopeKind::Function);
                    ast::visitor::preorder::walk_stmt(builder, stmt);
                    builder.pop_scope();
                });
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.split('.').next().unwrap()
                    };

                    let module = ModuleName::new(&alias.name.id);

                    let def = Definition::Import(ImportDefinition {
                        module: module.clone(),
                    });
                    self.add_symbol_with_def(symbol_name, def);
                    self.table.dependencies.push(Dependency::Module(module));
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                ..
            }) => {
                let module = module.as_ref().map(|m| ModuleName::new(&m.id));

                for alias in names {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.as_str()
                    };
                    let def = Definition::ImportFrom(ImportFromDefinition {
                        module: module.clone(),
                        name: Name::new(&alias.name.id),
                        level: *level,
                    });
                    self.add_symbol_with_def(symbol_name, def);
                }

                let dependency = if let Some(module) = module {
                    match NonZeroU32::new(*level) {
                        Some(level) => Dependency::Relative {
                            level,
                            module: Some(module),
                        },
                        None => Dependency::Module(module),
                    }
                } else {
                    Dependency::Relative {
                        level: NonZeroU32::new(*level)
                            .expect("Import without a module to have a level > 0"),
                        module,
                    }
                };

                self.table.dependencies.push(dependency);
            }
            _ => {
                ast::visitor::preorder::walk_stmt(self, stmt);
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct SymbolTablesStorage(KeyValueCache<FileId, Arc<SymbolTable>>);

impl Deref for SymbolTablesStorage {
    type Target = KeyValueCache<FileId, Arc<SymbolTable>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SymbolTablesStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use textwrap::dedent;

    use crate::parse::Parsed;
    use crate::symbols::ScopeKind;

    use super::{SymbolId, SymbolIterator, SymbolTable};

    mod from_ast {
        use super::*;

        fn parse(code: &str) -> Parsed {
            Parsed::from_text(&dedent(code))
        }

        fn names<I>(it: SymbolIterator<I>) -> Vec<&str>
        where
            I: Iterator<Item = SymbolId>,
        {
            let mut symbols: Vec<_> = it.map(|sym| sym.name.as_str()).collect();
            symbols.sort_unstable();
            symbols
        }

        #[test]
        fn empty() {
            let parsed = parse("");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()).len(), 0);
        }

        #[test]
        fn simple() {
            let parsed = parse("x");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["x"]);
            assert_eq!(
                table.defs(table.root_symbol_id_by_name("x").unwrap()).len(),
                0
            );
        }

        #[test]
        fn annotation_only() {
            let parsed = parse("x: int");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["int", "x"]);
            // TODO record definition
        }

        #[test]
        fn import() {
            let parsed = parse("import foo");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
            assert_eq!(
                table
                    .defs(table.root_symbol_id_by_name("foo").unwrap())
                    .len(),
                1
            );
        }

        #[test]
        fn import_sub() {
            let parsed = parse("import foo.bar");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
        }

        #[test]
        fn import_as() {
            let parsed = parse("import foo.bar as baz");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["baz"]);
        }

        #[test]
        fn import_from() {
            let parsed = parse("from bar import foo");
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
            assert_eq!(
                table
                    .defs(table.root_symbol_id_by_name("foo").unwrap())
                    .len(),
                1
            );
        }

        #[test]
        fn class_scope() {
            let parsed = parse(
                "
                class C:
                    x = 1
                y = 2
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["C", "y"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let c_scope = scopes[0].scope(&table);
            assert_eq!(c_scope.kind(), ScopeKind::Class);
            assert_eq!(c_scope.name(), "C");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
            assert_eq!(
                table.defs(table.root_symbol_id_by_name("C").unwrap()).len(),
                1
            );
        }

        #[test]
        fn func_scope() {
            let parsed = parse(
                "
                def func():
                    x = 1
                y = 2
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["func", "y"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let func_scope = scopes[0].scope(&table);
            assert_eq!(func_scope.kind(), ScopeKind::Function);
            assert_eq!(func_scope.name(), "func");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
            assert_eq!(
                table
                    .defs(table.root_symbol_id_by_name("func").unwrap())
                    .len(),
                1
            );
        }

        #[test]
        fn dupes() {
            let parsed = parse(
                "
                def func():
                    x = 1
                def func():
                    y = 2
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["func"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 2);
            let func_scope_1 = scopes[0].scope(&table);
            let func_scope_2 = scopes[1].scope(&table);
            assert_eq!(func_scope_1.kind(), ScopeKind::Function);
            assert_eq!(func_scope_1.name(), "func");
            assert_eq!(func_scope_2.kind(), ScopeKind::Function);
            assert_eq!(func_scope_2.name(), "func");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
            assert_eq!(names(table.symbols_for_scope(scopes[1])), vec!["y"]);
            assert_eq!(
                table
                    .defs(table.root_symbol_id_by_name("func").unwrap())
                    .len(),
                2
            );
        }

        #[test]
        fn generic_func() {
            let parsed = parse(
                "
                def func[T]():
                    x = 1
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["func"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let ann_scope_id = scopes[0];
            let ann_scope = ann_scope_id.scope(&table);
            assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
            assert_eq!(ann_scope.name(), "func");
            assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
            let scopes = table.child_scope_ids_of(ann_scope_id);
            assert_eq!(scopes.len(), 1);
            let func_scope_id = scopes[0];
            let func_scope = func_scope_id.scope(&table);
            assert_eq!(func_scope.kind(), ScopeKind::Function);
            assert_eq!(func_scope.name(), "func");
            assert_eq!(names(table.symbols_for_scope(func_scope_id)), vec!["x"]);
        }

        #[test]
        fn generic_class() {
            let parsed = parse(
                "
                class C[T]:
                    x = 1
                ",
            );
            let table = SymbolTable::from_ast(parsed.ast());
            assert_eq!(names(table.root_symbols()), vec!["C"]);
            let scopes = table.root_child_scope_ids();
            assert_eq!(scopes.len(), 1);
            let ann_scope_id = scopes[0];
            let ann_scope = ann_scope_id.scope(&table);
            assert_eq!(ann_scope.kind(), ScopeKind::Annotation);
            assert_eq!(ann_scope.name(), "C");
            assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
            let scopes = table.child_scope_ids_of(ann_scope_id);
            assert_eq!(scopes.len(), 1);
            let func_scope_id = scopes[0];
            let func_scope = func_scope_id.scope(&table);
            assert_eq!(func_scope.kind(), ScopeKind::Class);
            assert_eq!(func_scope.name(), "C");
            assert_eq!(names(table.symbols_for_scope(func_scope_id)), vec!["x"]);
        }
    }

    #[test]
    fn insert_same_name_symbol_twice() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let symbol_id_1 = table.add_symbol_to_scope(root_scope_id, "foo");
        let symbol_id_2 = table.add_symbol_to_scope(root_scope_id, "foo");
        assert_eq!(symbol_id_1, symbol_id_2);
    }

    #[test]
    fn insert_different_named_symbols() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let symbol_id_1 = table.add_symbol_to_scope(root_scope_id, "foo");
        let symbol_id_2 = table.add_symbol_to_scope(root_scope_id, "bar");
        assert_ne!(symbol_id_1, symbol_id_2);
    }

    #[test]
    fn add_child_scope_with_symbol() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_top = table.add_symbol_to_scope(root_scope_id, "foo");
        let c_scope = table.add_child_scope(root_scope_id, "C", ScopeKind::Class);
        let foo_symbol_inner = table.add_symbol_to_scope(c_scope, "foo");
        assert_ne!(foo_symbol_top, foo_symbol_inner);
    }

    #[test]
    fn scope_from_id() {
        let table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let scope = root_scope_id.scope(&table);
        assert_eq!(scope.name.as_str(), "<module>");
        assert_eq!(scope.kind, ScopeKind::Module);
    }

    #[test]
    fn symbol_from_id() {
        let mut table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let foo_symbol_id = table.add_symbol_to_scope(root_scope_id, "foo");
        let symbol = foo_symbol_id.symbol(&table);
        assert_eq!(symbol.name.as_str(), "foo");
    }
}
