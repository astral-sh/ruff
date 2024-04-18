#![allow(dead_code, unused_imports)]
use super::Name;
use hashbrown::hash_map::RawEntryMut;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::visitor::preorder::{PreorderVisitor, TraversalSignal};
use ruff_python_ast::visitor::{preorder, Visitor};
use ruff_python_ast::{AnyNodeRef, Expr, Mod, ModModule, Stmt, StmtExpr};
use ruff_python_parser::{Mode, ParseError};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

struct SourceText {
    text: String,
}

struct Parsed {
    ast: ModModule,
    imports: Vec<String>,
    errors: Vec<ParseError>,
}

impl Parsed {
    fn new(ast: ModModule, imports: Vec<String>, errors: Vec<ParseError>) -> Self {
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
        Ok(Mod::Module(module)) => (module, vec![]),
        Ok(Mod::Expression(expression)) => (
            ModModule {
                range: expression.range(),
                body: vec![Stmt::Expr(StmtExpr {
                    range: expression.range(),
                    value: expression.body,
                })],
            },
            vec![],
        ),
        Err(errors) => (
            ModModule {
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

enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
}

struct Scope {
    kind: ScopeKind,
    // symbol IDs, hashed by symbol name
    symbols_by_name: Map<SymbolId, ()>,
}

struct Symbol {
    name: Name,
    child_scopes: Vec<ScopeId>,
}

struct SymbolTable {
    scopes_by_id: IndexVec<ScopeId, Scope>,
    symbols_by_id: IndexVec<SymbolId, Symbol>,
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

    pub(crate) fn root_scope_id(&self) -> ScopeId {
        ScopeId::from_usize(0)
    }

    pub(crate) fn root_scope(&self) -> &Scope {
        &self.scopes_by_id[self.root_scope_id()]
    }

    pub(crate) fn add_symbol_to_scope(&mut self, scope_id: ScopeId, name: &str) -> SymbolId {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        let hash = hasher.finish();

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
