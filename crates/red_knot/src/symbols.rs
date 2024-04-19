#![allow(dead_code, unused_imports)]
use super::Name;
use hashbrown::hash_map::{Keys, RawEntryMut};
use itertools::Itertools;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::visitor::{preorder, Visitor};
use ruff_python_ast::{self as ast};
use ruff_python_parser::{Mode, ParseError};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};
use std::iter::{Copied, DoubleEndedIterator, FusedIterator};

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

struct SourceText {
    text: String,
}

struct Parsed {
    ast: ast::ModModule,
    imports: Vec<String>,
    errors: Vec<ParseError>,
}

impl Parsed {
    fn new(ast: ast::ModModule, imports: Vec<String>, errors: Vec<ParseError>) -> Self {
        Self {
            ast,
            imports,
            errors,
        }
    }
}

fn parse(source: &SourceText) -> Parsed {
    let result = ruff_python_parser::parse(&source.text, Mode::Module);

    let (module, errors) = match result {
        Ok(ast::Mod::Module(module)) => (module, vec![]),
        Ok(ast::Mod::Expression(expression)) => (
            ast::ModModule {
                range: expression.range(),
                body: vec![ast::Stmt::Expr(ast::StmtExpr {
                    range: expression.range(),
                    value: expression.body,
                })],
            },
            vec![],
        ),
        Err(errors) => (
            ast::ModModule {
                range: TextRange::default(),
                body: Vec::new(),
            },
            vec![errors],
        ),
    };

    Parsed::new(module, Vec::new(), errors)
}

#[newtype_index]
struct ScopeId;

#[newtype_index]
struct SymbolId;

#[derive(Debug, PartialEq)]
enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

struct Scope {
    pub name: Name,
    pub kind: ScopeKind,
    pub child_scopes: Vec<ScopeId>,
    // symbol IDs, hashed by symbol name
    symbols_by_name: Map<SymbolId, ()>,
}

struct Symbol {
    pub name: Name,
}

struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
}

struct SymbolIterator<'a, I> {
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

    pub(crate) fn from_ast(module: &ast::ModModule) -> SymbolTable {
        let table = SymbolTable::new();
        let root_scope_id = SymbolTable::root_scope_id();
        let mut builder = SymbolTableBuilder {
            table,
            scopes: vec![root_scope_id],
        };
        builder.visit_body(&module.body);
        builder.table
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

struct SymbolTableBuilder {
    table: SymbolTable,
    scopes: Vec<ScopeId>,
}

impl SymbolTableBuilder {
    fn add_symbol(&mut self, identifier: &str) -> SymbolId {
        self.table.add_symbol_to_scope(self.cur_scope(), identifier)
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
}

impl<'a> PreorderVisitor<'a> for SymbolTableBuilder {
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if let ast::Expr::Name(ast::ExprName { id, .. }) = expr {
            self.add_symbol(id);
        }
        ast::visitor::preorder::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef { name, .. }) => {
                self.add_symbol(name);
                self.push_scope(self.cur_scope(), name, ScopeKind::Class);
                ast::visitor::preorder::walk_stmt(self, stmt);
                self.pop_scope();
            }
            ast::Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => {
                self.add_symbol(name);
                self.push_scope(self.cur_scope(), name, ScopeKind::Function);
                ast::visitor::preorder::walk_stmt(self, stmt);
                self.pop_scope();
            }
            _ => {
                ast::visitor::preorder::walk_stmt(self, stmt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use textwrap::dedent;

    mod from_ast {
        use super::*;

        fn build(code: &str) -> SymbolTable {
            let source_text = SourceText { text: dedent(code) };
            let parsed = parse(&source_text);
            SymbolTable::from_ast(&parsed.ast)
        }

        fn names<I>(it: SymbolIterator<I>) -> Vec<&str>
        where
            I: Iterator<Item = SymbolId>,
        {
            it.map(|sym| sym.name.0.as_str()).sorted().collect()
        }

        #[test]
        fn empty() {
            let table = build("");
            assert_eq!(names(table.root_symbols()).len(), 0);
        }

        #[test]
        fn simple() {
            let table = build("x");
            assert_eq!(names(table.root_symbols()), vec!["x"]);
        }

        #[test]
        fn annotation_only() {
            let table = build("x: int");
            assert_eq!(names(table.root_symbols()), vec!["int", "x"]);
        }

        #[test]
        fn class_scope() {
            let table = build(
                "
                class C:
                    x = 1
                y = 2
                ",
            );
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
            let table = build(
                "
                def func():
                    x = 1
                y = 2
                ",
            );
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
            let table = build(
                "
                def func():
                    x = 1
                def func():
                    y = 2
                ",
            );
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
