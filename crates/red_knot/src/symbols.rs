#![allow(dead_code, unused_imports)]
use super::Name;
use hashbrown::hash_map::RawEntryMut;
use itertools::Itertools;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::visitor::{preorder, Visitor};
use ruff_python_ast::{self as ast};
use ruff_python_parser::{Mode, ParseError};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

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

fn parse(source: SourceText) -> Parsed {
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
    pub kind: ScopeKind,
    // symbol IDs, hashed by symbol name
    symbols_by_name: Map<SymbolId, ()>,
}

struct Symbol {
    pub name: Name,
    pub child_scopes: Vec<ScopeId>,
}

struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
}

struct SymbolIterator<'a> {
    table: &'a SymbolTable,
    ids: Vec<SymbolId>,
}

impl<'a> Iterator for SymbolIterator<'a> {
    type Item = &'a Symbol;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.ids.pop()?;
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
            kind: ScopeKind::Module,
            symbols_by_name: Map::default(),
        });
        table
    }

    pub(crate) fn from_ast(module: ast::ModModule) -> SymbolTable {
        let table = SymbolTable::new();
        let root_scope_id = table.root_scope_id();
        let mut builder = SymbolTableBuilder {
            table,
            scopes: vec![root_scope_id],
        };
        builder.visit_body(&module.body);
        builder.table
    }

    pub(crate) fn root_scope_id(&self) -> ScopeId {
        ScopeId::from_usize(0)
    }

    pub(crate) fn root_scope(&self) -> &Scope {
        &self.scopes_by_id[self.root_scope_id()]
    }

    pub(crate) fn symbols_for_scope(&self, scope_id: ScopeId) -> SymbolIterator {
        let scope = &self.scopes_by_id[scope_id];
        SymbolIterator {
            table: self,
            ids: scope.symbols_by_name.keys().copied().collect(),
        }
    }

    pub(crate) fn root_symbols(&self) -> SymbolIterator {
        self.symbols_for_scope(self.root_scope_id())
    }

    pub(crate) fn symbol_by_name(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        let scope = &self.scopes_by_id[scope_id];
        let hash = self.hash_name(name);
        let name = Name::new(name);
        scope
            .symbols_by_name
            .raw_entry()
            .from_hash(hash, |symid| self.symbols_by_id[*symid].name == name)
            .map(|(k, ())| &self.symbols_by_id[*k])
    }

    pub(crate) fn add_symbol_to_scope(&mut self, scope_id: ScopeId, name: &str) -> SymbolId {
        let hash = self.hash_name(name);
        let scope = &mut self.scopes_by_id[scope_id];
        let name = Name::new(name);

        let entry = scope
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |existing| self.symbols_by_id[*existing].name == name);

        match entry {
            RawEntryMut::Occupied(entry) => *entry.key(),
            RawEntryMut::Vacant(entry) => {
                let symbol = Symbol {
                    name,
                    child_scopes: Vec::new(),
                };
                let id = self.symbols_by_id.push(symbol);
                entry.insert_with_hasher(hash, id, (), |_| hash);
                id
            }
        }
    }

    pub(crate) fn add_child_scope_to_symbol(
        &mut self,
        symbol_id: SymbolId,
        kind: ScopeKind,
    ) -> ScopeId {
        let scope_id = self.scopes_by_id.push(Scope {
            kind,
            symbols_by_name: Map::default(),
        });
        let symbol = &mut self.symbols_by_id[symbol_id];
        symbol.child_scopes.push(scope_id);
        scope_id
    }

    fn hash_name(&self, name: &str) -> u64 {
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

    fn push_scope(&mut self, child_of: SymbolId, kind: ScopeKind) -> ScopeId {
        let scope_id = self.table.add_child_scope_to_symbol(child_of, kind);
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
        match expr {
            ast::Expr::Name(ast::ExprName { id, .. }) => {
                self.add_symbol(id);
            }
            _ => {}
        }
        ast::visitor::preorder::walk_expr(self, expr);
    }

    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef { name, .. }) => {
                let symbol_id = self.add_symbol(name);
                self.push_scope(symbol_id, ScopeKind::Class);
                ast::visitor::preorder::walk_stmt(self, stmt);
                self.pop_scope();
            }
            ast::Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => {
                let symbol_id = self.add_symbol(name);
                self.push_scope(symbol_id, ScopeKind::Function);
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
            let parsed = parse(source_text);
            SymbolTable::from_ast(parsed.ast)
        }

        fn names(it: SymbolIterator) -> Vec<&str> {
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
            let sym_c = table.symbol_by_name(table.root_scope_id(), "C").unwrap();
            assert_eq!(sym_c.child_scopes.len(), 1);
            let c_scope = sym_c.child_scopes[0];
            assert_eq!(table.scopes_by_id[c_scope].kind, ScopeKind::Class);
            assert_eq!(names(table.symbols_for_scope(c_scope)), vec!["x"]);
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
            let sym_func = table.symbol_by_name(table.root_scope_id(), "func").unwrap();
            assert_eq!(sym_func.child_scopes.len(), 1);
            let func_scope = sym_func.child_scopes[0];
            assert_eq!(table.scopes_by_id[func_scope].kind, ScopeKind::Function);
            assert_eq!(names(table.symbols_for_scope(func_scope)), vec!["x"]);
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
            let sym_func = table.symbol_by_name(table.root_scope_id(), "func").unwrap();
            assert_eq!(sym_func.child_scopes.len(), 2);
            let func_scope_1 = sym_func.child_scopes[0];
            let func_scope_2 = sym_func.child_scopes[1];
            assert_eq!(table.scopes_by_id[func_scope_1].kind, ScopeKind::Function);
            assert_eq!(table.scopes_by_id[func_scope_2].kind, ScopeKind::Function);
            assert_eq!(names(table.symbols_for_scope(func_scope_1)), vec!["x"]);
            assert_eq!(names(table.symbols_for_scope(func_scope_2)), vec!["y"]);
        }
    }

    #[test]
    fn insert_same_name_symbol_twice() {
        let mut table = SymbolTable::new();
        let root_scope_id = table.root_scope_id();
        let symbol_id_1 = table.add_symbol_to_scope(root_scope_id, "foo");
        let symbol_id_2 = table.add_symbol_to_scope(root_scope_id, "foo");
        assert_eq!(symbol_id_1, symbol_id_2);
    }

    #[test]
    fn insert_different_named_symbols() {
        let mut table = SymbolTable::new();
        let root_scope_id = table.root_scope_id();
        let symbol_id_1 = table.add_symbol_to_scope(root_scope_id, "foo");
        let symbol_id_2 = table.add_symbol_to_scope(root_scope_id, "bar");
        assert_ne!(symbol_id_1, symbol_id_2);
    }

    #[test]
    fn add_child_scope_with_symbol() {
        let mut table = SymbolTable::new();
        let root_scope_id = table.root_scope_id();
        let foo_symbol_top = table.add_symbol_to_scope(root_scope_id, "foo");
        let c_symbol = table.add_symbol_to_scope(root_scope_id, "C");
        let c_scope = table.add_child_scope_to_symbol(c_symbol, ScopeKind::Class);
        let foo_symbol_inner = table.add_symbol_to_scope(c_scope, "foo");
        assert_ne!(foo_symbol_top, foo_symbol_inner);
    }
}
