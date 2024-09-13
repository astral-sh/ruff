use std::hash::{Hash, Hasher};
use std::ops::Range;

use bitflags::bitflags;
use hashbrown::hash_map::RawEntryMut;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast};
use rustc_hash::FxHasher;

use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::{semantic_index, SymbolMap};
use crate::Db;

#[derive(Eq, PartialEq, Debug)]
pub struct Symbol {
    name: Name,
    flags: SymbolFlags,
}

impl Symbol {
    fn new(name: Name) -> Self {
        Self {
            name,
            flags: SymbolFlags::empty(),
        }
    }

    fn insert_flags(&mut self, flags: SymbolFlags) {
        self.flags.insert(flags);
    }

    /// The symbol's name.
    pub fn name(&self) -> &Name {
        &self.name
    }

    /// Is the symbol used in its containing scope?
    pub fn is_used(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_USED)
    }

    /// Is the symbol defined in its containing scope?
    pub fn is_bound(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_BOUND)
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct SymbolFlags: u8 {
        const IS_USED         = 1 << 0;
        const IS_BOUND      = 1 << 1;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL   = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL = 1 << 3;
    }
}

/// ID that uniquely identifies a symbol in a file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FileSymbolId {
    scope: FileScopeId,
    scoped_symbol_id: ScopedSymbolId,
}

impl FileSymbolId {
    pub fn scope(self) -> FileScopeId {
        self.scope
    }

    pub(crate) fn scoped_symbol_id(self) -> ScopedSymbolId {
        self.scoped_symbol_id
    }
}

impl From<FileSymbolId> for ScopedSymbolId {
    fn from(val: FileSymbolId) -> Self {
        val.scoped_symbol_id()
    }
}

/// Symbol ID that uniquely identifies a symbol inside a [`Scope`].
#[newtype_index]
pub struct ScopedSymbolId;

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked]
pub struct ScopeId<'db> {
    #[id]
    pub file: File,
    #[id]
    pub file_scope_id: FileScopeId,

    /// The node that introduces this scope.
    #[no_eq]
    #[return_ref]
    pub node: NodeWithScopeKind,

    #[no_eq]
    count: countme::Count<ScopeId<'static>>,
}

impl<'db> ScopeId<'db> {
    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        // Type parameter scopes behave like function scopes in terms of name resolution; CPython
        // symbol table also uses the term "function-like" for these scopes.
        matches!(
            self.node(db),
            NodeWithScopeKind::ClassTypeParameters(_)
                | NodeWithScopeKind::FunctionTypeParameters(_)
                | NodeWithScopeKind::Function(_)
                | NodeWithScopeKind::ListComprehension(_)
                | NodeWithScopeKind::SetComprehension(_)
                | NodeWithScopeKind::DictComprehension(_)
                | NodeWithScopeKind::GeneratorExpression(_)
        )
    }

    #[cfg(test)]
    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self.node(db) {
            NodeWithScopeKind::Module => "<module>",
            NodeWithScopeKind::Class(class) | NodeWithScopeKind::ClassTypeParameters(class) => {
                class.name.as_str()
            }
            NodeWithScopeKind::Function(function)
            | NodeWithScopeKind::FunctionTypeParameters(function) => function.name.as_str(),
            NodeWithScopeKind::Lambda(_) => "<lambda>",
            NodeWithScopeKind::ListComprehension(_) => "<listcomp>",
            NodeWithScopeKind::SetComprehension(_) => "<setcomp>",
            NodeWithScopeKind::DictComprehension(_) => "<dictcomp>",
            NodeWithScopeKind::GeneratorExpression(_) => "<generator>",
        }
    }
}

/// ID that uniquely identifies a scope inside of a module.
#[newtype_index]
pub struct FileScopeId;

impl FileScopeId {
    /// Returns the scope id of the module-global scope.
    pub fn global() -> Self {
        FileScopeId::from_u32(0)
    }

    pub fn is_global(self) -> bool {
        self == FileScopeId::global()
    }

    pub fn to_scope_id(self, db: &dyn Db, file: File) -> ScopeId<'_> {
        let index = semantic_index(db, file);
        index.scope_ids_by_scope[self]
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Scope {
    pub(super) parent: Option<FileScopeId>,
    pub(super) kind: ScopeKind,
    pub(super) descendents: Range<FileScopeId>,
}

impl Scope {
    pub fn parent(self) -> Option<FileScopeId> {
        self.parent
    }

    pub fn kind(&self) -> ScopeKind {
        self.kind
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
    Comprehension,
}

impl ScopeKind {
    pub const fn is_comprehension(self) -> bool {
        matches!(self, ScopeKind::Comprehension)
    }
}

/// Symbol table for a specific [`Scope`].
#[derive(Debug)]
pub struct SymbolTable {
    /// The symbols in this scope.
    symbols: IndexVec<ScopedSymbolId, Symbol>,

    /// The symbols indexed by name.
    symbols_by_name: SymbolMap,
}

impl SymbolTable {
    fn new() -> Self {
        Self {
            symbols: IndexVec::new(),
            symbols_by_name: SymbolMap::default(),
        }
    }

    fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
    }

    pub(crate) fn symbol(&self, symbol_id: impl Into<ScopedSymbolId>) -> &Symbol {
        &self.symbols[symbol_id.into()]
    }

    #[allow(unused)]
    pub(crate) fn symbol_ids(&self) -> impl Iterator<Item = ScopedSymbolId> {
        self.symbols.indices()
    }

    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    /// Returns the symbol named `name`.
    pub(crate) fn symbol_by_name(&self, name: &str) -> Option<&Symbol> {
        let id = self.symbol_id_by_name(name)?;
        Some(self.symbol(id))
    }

    /// Returns the [`ScopedSymbolId`] of the symbol named `name`.
    pub(crate) fn symbol_id_by_name(&self, name: &str) -> Option<ScopedSymbolId> {
        let (id, ()) = self
            .symbols_by_name
            .raw_entry()
            .from_hash(Self::hash_name(name), |id| {
                self.symbol(*id).name().as_str() == name
            })?;

        Some(*id)
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

impl PartialEq for SymbolTable {
    fn eq(&self, other: &Self) -> bool {
        // We don't need to compare the symbols_by_name because the name is already captured in `Symbol`.
        self.symbols == other.symbols
    }
}

impl Eq for SymbolTable {}

#[derive(Debug)]
pub(super) struct SymbolTableBuilder {
    table: SymbolTable,
}

impl SymbolTableBuilder {
    pub(super) fn new() -> Self {
        Self {
            table: SymbolTable::new(),
        }
    }

    pub(super) fn add_symbol(&mut self, name: Name) -> (ScopedSymbolId, bool) {
        let hash = SymbolTable::hash_name(&name);
        let entry = self
            .table
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |id| self.table.symbols[*id].name() == &name);

        match entry {
            RawEntryMut::Occupied(entry) => (*entry.key(), false),
            RawEntryMut::Vacant(entry) => {
                let symbol = Symbol::new(name);

                let id = self.table.symbols.push(symbol);
                entry.insert_with_hasher(hash, id, (), |id| {
                    SymbolTable::hash_name(self.table.symbols[*id].name().as_str())
                });
                (id, true)
            }
        }
    }

    pub(super) fn mark_symbol_bound(&mut self, id: ScopedSymbolId) {
        self.table.symbols[id].insert_flags(SymbolFlags::IS_BOUND);
    }

    pub(super) fn mark_symbol_used(&mut self, id: ScopedSymbolId) {
        self.table.symbols[id].insert_flags(SymbolFlags::IS_USED);
    }

    pub(super) fn finish(mut self) -> SymbolTable {
        self.table.shrink_to_fit();
        self.table
    }
}

/// Reference to a node that introduces a new scope.
#[derive(Copy, Clone, Debug)]
pub(crate) enum NodeWithScopeRef<'a> {
    Module,
    Class(&'a ast::StmtClassDef),
    Function(&'a ast::StmtFunctionDef),
    Lambda(&'a ast::ExprLambda),
    FunctionTypeParameters(&'a ast::StmtFunctionDef),
    ClassTypeParameters(&'a ast::StmtClassDef),
    ListComprehension(&'a ast::ExprListComp),
    SetComprehension(&'a ast::ExprSetComp),
    DictComprehension(&'a ast::ExprDictComp),
    GeneratorExpression(&'a ast::ExprGenerator),
}

impl NodeWithScopeRef<'_> {
    /// Converts the unowned reference to an owned [`NodeWithScopeKind`].
    ///
    /// # Safety
    /// The node wrapped by `self` must be a child of `module`.
    #[allow(unsafe_code)]
    pub(super) unsafe fn to_kind(self, module: ParsedModule) -> NodeWithScopeKind {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKind::Module,
            NodeWithScopeRef::Class(class) => {
                NodeWithScopeKind::Class(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKind::Function(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::Lambda(lambda) => {
                NodeWithScopeKind::Lambda(AstNodeRef::new(module, lambda))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKind::FunctionTypeParameters(AstNodeRef::new(module, function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKind::ClassTypeParameters(AstNodeRef::new(module, class))
            }
            NodeWithScopeRef::ListComprehension(comprehension) => {
                NodeWithScopeKind::ListComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::SetComprehension(comprehension) => {
                NodeWithScopeKind::SetComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::DictComprehension(comprehension) => {
                NodeWithScopeKind::DictComprehension(AstNodeRef::new(module, comprehension))
            }
            NodeWithScopeRef::GeneratorExpression(generator) => {
                NodeWithScopeKind::GeneratorExpression(AstNodeRef::new(module, generator))
            }
        }
    }

    pub(super) fn scope_kind(self) -> ScopeKind {
        match self {
            NodeWithScopeRef::Module => ScopeKind::Module,
            NodeWithScopeRef::Class(_) => ScopeKind::Class,
            NodeWithScopeRef::Function(_) => ScopeKind::Function,
            NodeWithScopeRef::Lambda(_) => ScopeKind::Function,
            NodeWithScopeRef::FunctionTypeParameters(_)
            | NodeWithScopeRef::ClassTypeParameters(_) => ScopeKind::Annotation,
            NodeWithScopeRef::ListComprehension(_)
            | NodeWithScopeRef::SetComprehension(_)
            | NodeWithScopeRef::DictComprehension(_)
            | NodeWithScopeRef::GeneratorExpression(_) => ScopeKind::Comprehension,
        }
    }

    pub(crate) fn node_key(self) -> NodeWithScopeKey {
        match self {
            NodeWithScopeRef::Module => NodeWithScopeKey::Module,
            NodeWithScopeRef::Class(class) => NodeWithScopeKey::Class(NodeKey::from_node(class)),
            NodeWithScopeRef::Function(function) => {
                NodeWithScopeKey::Function(NodeKey::from_node(function))
            }
            NodeWithScopeRef::Lambda(lambda) => {
                NodeWithScopeKey::Lambda(NodeKey::from_node(lambda))
            }
            NodeWithScopeRef::FunctionTypeParameters(function) => {
                NodeWithScopeKey::FunctionTypeParameters(NodeKey::from_node(function))
            }
            NodeWithScopeRef::ClassTypeParameters(class) => {
                NodeWithScopeKey::ClassTypeParameters(NodeKey::from_node(class))
            }
            NodeWithScopeRef::ListComprehension(comprehension) => {
                NodeWithScopeKey::ListComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::SetComprehension(comprehension) => {
                NodeWithScopeKey::SetComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::DictComprehension(comprehension) => {
                NodeWithScopeKey::DictComprehension(NodeKey::from_node(comprehension))
            }
            NodeWithScopeRef::GeneratorExpression(generator) => {
                NodeWithScopeKey::GeneratorExpression(NodeKey::from_node(generator))
            }
        }
    }
}

/// Node that introduces a new scope.
#[derive(Clone, Debug)]
pub enum NodeWithScopeKind {
    Module,
    Class(AstNodeRef<ast::StmtClassDef>),
    ClassTypeParameters(AstNodeRef<ast::StmtClassDef>),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    FunctionTypeParameters(AstNodeRef<ast::StmtFunctionDef>),
    Lambda(AstNodeRef<ast::ExprLambda>),
    ListComprehension(AstNodeRef<ast::ExprListComp>),
    SetComprehension(AstNodeRef<ast::ExprSetComp>),
    DictComprehension(AstNodeRef<ast::ExprDictComp>),
    GeneratorExpression(AstNodeRef<ast::ExprGenerator>),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum NodeWithScopeKey {
    Module,
    Class(NodeKey),
    ClassTypeParameters(NodeKey),
    Function(NodeKey),
    FunctionTypeParameters(NodeKey),
    Lambda(NodeKey),
    ListComprehension(NodeKey),
    SetComprehension(NodeKey),
    DictComprehension(NodeKey),
    GeneratorExpression(NodeKey),
}
