#![allow(dead_code)]

use std::hash::{Hash, Hasher};
use std::iter::{Copied, DoubleEndedIterator, FusedIterator};

use hashbrown::hash_map::{Keys, RawEntryMut};
use rustc_hash::FxHasher;

use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast as ast;
use ruff_python_ast::visitor::preorder::PreorderVisitor;

use crate::{FxDashMap, Name};

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

#[newtype_index]
pub(crate) struct ScopeId;

#[newtype_index]
pub(crate) struct SymbolId;

#[derive(Debug, PartialEq)]
pub(crate) enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

pub(crate) struct Scope {
    pub(crate) name: Name,
    pub(crate) kind: ScopeKind,
    pub(crate) child_scopes: Vec<ScopeId>,
    // symbol IDs, hashed by symbol name
    symbols_by_name: Map<SymbolId, ()>,
}

pub(crate) struct Symbol {
    pub(crate) name: Name,
}

pub(crate) struct Symbols<'a> {
    table: SymbolTable,
    defs: SymbolDefs<'a>,
}

/// Table of all symbols in all scopes for a module.
/// Derived from module AST, but holds no references to it.
pub(crate) struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
}

/// Maps Symbol Id to its definitions (as AST Stmt references)
#[derive(Default)]
pub(crate) struct SymbolDefs<'a> {
    definitions: FxDashMap<SymbolId, Vec<&'a ast::Stmt>>,
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

impl<'a> Symbols<'a> {
    pub(crate) fn from_ast(module: &'a ast::ModModule) -> Self {
        let symbols = Symbols {
            table: SymbolTable::new(),
            defs: SymbolDefs::default(),
        };
        let root_scope_id = SymbolTable::root_scope_id();
        let mut builder = SymbolsBuilder {
            symbols,
            scopes: vec![root_scope_id],
        };
        builder.visit_body(&module.body);
        builder.symbols
    }
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        let mut table = SymbolTable {
            scopes_by_id: IndexVec::new(),
            symbols_by_id: IndexVec::new(),
        };
        table.scopes_by_id.push(Scope {
            name: Name::new("<module>"),
            kind: ScopeKind::Module,
            child_scopes: Vec::new(),
            symbols_by_name: Map::default(),
        });
        table
    }

    pub(crate) fn root_scope_id() -> ScopeId {
        ScopeId::from_usize(0)
    }

    pub(crate) fn root_scope(&self) -> &Scope {
        &self.scopes_by_id[SymbolTable::root_scope_id()]
    }

    pub(crate) fn symbols_for_scope(
        &self,
        scope_id: ScopeId,
    ) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        let scope = &self.scopes_by_id[scope_id];
        SymbolIterator {
            table: self,
            ids: scope.symbols_by_name.keys().copied(),
        }
    }

    pub(crate) fn root_symbols(&self) -> SymbolIterator<Copied<Keys<SymbolId, ()>>> {
        self.symbols_for_scope(SymbolTable::root_scope_id())
    }

    pub(crate) fn child_scopes_of(&self, scope_id: ScopeId) -> &[ScopeId] {
        &self.scopes_by_id[scope_id].child_scopes
    }

    pub(crate) fn root_child_scopes(&self) -> &[ScopeId] {
        self.child_scopes_of(SymbolTable::root_scope_id())
    }

    pub(crate) fn symbol_by_name(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        let scope = &self.scopes_by_id[scope_id];
        let hash = SymbolTable::hash_name(name);
        let name = Name::new(name);
        scope
            .symbols_by_name
            .raw_entry()
            .from_hash(hash, |symid| self.symbols_by_id[*symid].name == name)
            .map(|(k, ())| &self.symbols_by_id[*k])
    }

    pub(crate) fn add_symbol_to_scope(&mut self, scope_id: ScopeId, name: &str) -> SymbolId {
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

    pub(crate) fn add_child_scope(
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

struct SymbolsBuilder<'a> {
    symbols: Symbols<'a>,
    scopes: Vec<ScopeId>,
}

impl<'a> SymbolsBuilder<'a> {
    fn add_symbol(&mut self, identifier: &str) -> SymbolId {
        self.symbols
            .table
            .add_symbol_to_scope(self.cur_scope(), identifier)
    }

    fn add_symbol_with_def(&mut self, identifier: &str, node: &'a ast::Stmt) -> SymbolId {
        let symbol_id = self.add_symbol(identifier);
        self.symbols
            .defs
            .definitions
            .entry(symbol_id)
            .or_default()
            .push(node);
        symbol_id
    }

    fn push_scope(&mut self, child_of: ScopeId, name: &str, kind: ScopeKind) -> ScopeId {
        let scope_id = self.symbols.table.add_child_scope(child_of, name, kind);
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

impl<'a> PreorderVisitor<'a> for SymbolsBuilder<'a> {
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = expr {
            self.add_symbol(id);
        }
        ast::visitor::preorder::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        // TODO need to capture more definition statements here
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef {
                name, type_params, ..
            }) => {
                self.add_symbol_with_def(name, stmt);
                self.with_type_params(name, type_params, |builder| {
                    builder.push_scope(builder.cur_scope(), name, ScopeKind::Class);
                    ast::visitor::preorder::walk_stmt(builder, stmt);
                    builder.pop_scope();
                });
            }
            ast::Stmt::FunctionDef(ast::StmtFunctionDef {
                name, type_params, ..
            }) => {
                self.add_symbol_with_def(name, stmt);
                self.with_type_params(name, type_params, |builder| {
                    builder.push_scope(builder.cur_scope(), name, ScopeKind::Function);
                    ast::visitor::preorder::walk_stmt(builder, stmt);
                    builder.pop_scope();
                });
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for alias in names {
                    self.add_symbol_with_def(alias.name.id.split('.').next().unwrap(), stmt);
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
                for alias in names {
                    self.add_symbol_with_def(&alias.name.id, stmt);
                }
            }
            _ => {
                ast::visitor::preorder::walk_stmt(self, stmt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use textwrap::dedent;

    use crate::parse::Parsed;
    use crate::symbols::ScopeKind;

    use super::{SymbolId, SymbolIterator, SymbolTable, Symbols};

    mod from_ast {
        use super::*;

        fn parse(code: &str) -> Parsed {
            Parsed::from_text(&dedent(code))
        }

        fn names<I>(it: SymbolIterator<I>) -> Vec<&str>
        where
            I: Iterator<Item = SymbolId>,
        {
            it.map(|sym| sym.name.0.as_str()).sorted().collect()
        }

        #[test]
        fn empty() {
            let parsed = parse("");
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()).len(), 0);
        }

        #[test]
        fn simple() {
            let parsed = parse("x");
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["x"]);
        }

        #[test]
        fn annotation_only() {
            let parsed = parse("x: int");
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["int", "x"]);
        }

        #[test]
        fn import() {
            let parsed = parse("import foo");
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
        }

        #[test]
        fn import_sub() {
            let parsed = parse("import foo.bar");
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
        }

        #[test]
        fn import_from() {
            let parsed = parse("from bar import foo");
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["foo"]);
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
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["C", "y"]);
            let scopes = table.root_child_scopes();
            assert_eq!(scopes.len(), 1);
            let c_scope = &table.scopes_by_id[scopes[0]];
            assert_eq!(c_scope.kind, ScopeKind::Class);
            assert_eq!(c_scope.name.as_str(), "C");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
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
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["func", "y"]);
            let scopes = table.root_child_scopes();
            assert_eq!(scopes.len(), 1);
            let func_scope = &table.scopes_by_id[scopes[0]];
            assert_eq!(func_scope.kind, ScopeKind::Function);
            assert_eq!(func_scope.name.as_str(), "func");
            assert_eq!(names(table.symbols_for_scope(scopes[0])), vec!["x"]);
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
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["func"]);
            let scopes = table.root_child_scopes();
            assert_eq!(scopes.len(), 2);
            let func_scope_1 = scopes[0];
            let func_scope_2 = scopes[1];
            assert_eq!(table.scopes_by_id[func_scope_1].kind, ScopeKind::Function);
            assert_eq!(table.scopes_by_id[func_scope_1].name.as_str(), "func");
            assert_eq!(table.scopes_by_id[func_scope_2].kind, ScopeKind::Function);
            assert_eq!(table.scopes_by_id[func_scope_2].name.as_str(), "func");
            assert_eq!(names(table.symbols_for_scope(func_scope_1)), vec!["x"]);
            assert_eq!(names(table.symbols_for_scope(func_scope_2)), vec!["y"]);
        }

        #[test]
        fn generic_func() {
            let parsed = parse(
                "
                def func[T]():
                    x = 1
                ",
            );
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["func"]);
            let scopes = table.root_child_scopes();
            assert_eq!(scopes.len(), 1);
            let ann_scope_id = scopes[0];
            let ann_scope = &table.scopes_by_id[ann_scope_id];
            assert_eq!(ann_scope.kind, ScopeKind::Annotation);
            assert_eq!(ann_scope.name.as_str(), "func");
            assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
            let scopes = table.child_scopes_of(ann_scope_id);
            assert_eq!(scopes.len(), 1);
            let func_scope_id = scopes[0];
            let func_scope = &table.scopes_by_id[func_scope_id];
            assert_eq!(func_scope.kind, ScopeKind::Function);
            assert_eq!(func_scope.name.as_str(), "func");
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
            let table = Symbols::from_ast(parsed.ast()).table;
            assert_eq!(names(table.root_symbols()), vec!["C"]);
            let scopes = table.root_child_scopes();
            assert_eq!(scopes.len(), 1);
            let ann_scope_id = scopes[0];
            let ann_scope = &table.scopes_by_id[ann_scope_id];
            assert_eq!(ann_scope.kind, ScopeKind::Annotation);
            assert_eq!(ann_scope.name.as_str(), "C");
            assert_eq!(names(table.symbols_for_scope(ann_scope_id)), vec!["T"]);
            let scopes = table.child_scopes_of(ann_scope_id);
            assert_eq!(scopes.len(), 1);
            let func_scope_id = scopes[0];
            let func_scope = &table.scopes_by_id[func_scope_id];
            assert_eq!(func_scope.kind, ScopeKind::Class);
            assert_eq!(func_scope.name.as_str(), "C");
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
}
