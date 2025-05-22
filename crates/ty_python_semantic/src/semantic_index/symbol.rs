use std::hash::{Hash, Hasher};
use std::ops::Range;

use bitflags::bitflags;
use hashbrown::hash_map::RawEntryMut;
use ruff_db::files::File;
use ruff_db::parsed::ParsedModule;
use ruff_index::{IndexVec, newtype_index};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHasher;

use crate::Db;
use crate::ast_node_ref::AstNodeRef;
use crate::node_key::NodeKey;
use crate::semantic_index::visibility_constraints::ScopedVisibilityConstraintId;
use crate::semantic_index::{SemanticIndex, SymbolMap, semantic_index};

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

    /// Is the symbol declared in its containing scope?
    pub fn is_declared(&self) -> bool {
        self.flags.contains(SymbolFlags::IS_DECLARED)
    }
}

bitflags! {
    /// Flags that can be queried to obtain information about a symbol in a given scope.
    ///
    /// See the doc-comment at the top of [`super::use_def`] for explanations of what it
    /// means for a symbol to be *bound* as opposed to *declared*.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    struct SymbolFlags: u8 {
        const IS_USED         = 1 << 0;
        const IS_BOUND        = 1 << 1;
        const IS_DECLARED     = 1 << 2;
        /// TODO: This flag is not yet set by anything
        const MARKED_GLOBAL   = 1 << 3;
        /// TODO: This flag is not yet set by anything
        const MARKED_NONLOCAL = 1 << 4;
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
#[derive(salsa::Update)]
pub struct ScopedSymbolId;

/// A cross-module identifier of a scope that can be used as a salsa query parameter.
#[salsa::tracked(debug)]
pub struct ScopeId<'db> {
    pub file: File,

    pub file_scope_id: FileScopeId,

    count: countme::Count<ScopeId<'static>>,
}

impl<'db> ScopeId<'db> {
    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        self.node(db).scope_kind().is_function_like()
    }

    pub(crate) fn is_type_parameter(self, db: &'db dyn Db) -> bool {
        self.node(db).scope_kind().is_type_parameter()
    }

    pub(crate) fn node(self, db: &dyn Db) -> &NodeWithScopeKind {
        self.scope(db).node()
    }

    pub(crate) fn scope(self, db: &dyn Db) -> &Scope {
        semantic_index(db, self.file(db)).scope(self.file_scope_id(db))
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
            NodeWithScopeKind::TypeAlias(type_alias)
            | NodeWithScopeKind::TypeAliasTypeParameters(type_alias) => type_alias
                .name
                .as_name_expr()
                .map(|name| name.id.as_str())
                .unwrap_or("<type alias>"),
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
#[derive(salsa::Update)]
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

    pub(crate) fn is_generator_function(self, index: &SemanticIndex) -> bool {
        index.generator_functions.contains(&self)
    }
}

#[derive(Debug, salsa::Update)]
pub struct Scope {
    parent: Option<FileScopeId>,
    node: NodeWithScopeKind,
    descendants: Range<FileScopeId>,
    reachability: ScopedVisibilityConstraintId,
}

impl Scope {
    pub(super) fn new(
        parent: Option<FileScopeId>,
        node: NodeWithScopeKind,
        descendants: Range<FileScopeId>,
        reachability: ScopedVisibilityConstraintId,
    ) -> Self {
        Scope {
            parent,
            node,
            descendants,
            reachability,
        }
    }

    pub fn parent(&self) -> Option<FileScopeId> {
        self.parent
    }

    pub fn node(&self) -> &NodeWithScopeKind {
        &self.node
    }

    pub fn kind(&self) -> ScopeKind {
        self.node().scope_kind()
    }

    pub fn descendants(&self) -> Range<FileScopeId> {
        self.descendants.clone()
    }

    pub(super) fn extend_descendants(&mut self, children_end: FileScopeId) {
        self.descendants = self.descendants.start..children_end;
    }

    pub(crate) fn is_eager(&self) -> bool {
        self.kind().is_eager()
    }

    pub(crate) fn reachability(&self) -> ScopedVisibilityConstraintId {
        self.reachability
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Annotation,
    Class,
    Function,
    Lambda,
    Comprehension,
    TypeAlias,
}

impl ScopeKind {
    pub(crate) fn is_eager(self) -> bool {
        match self {
            ScopeKind::Module | ScopeKind::Class | ScopeKind::Comprehension => true,
            ScopeKind::Annotation
            | ScopeKind::Function
            | ScopeKind::Lambda
            | ScopeKind::TypeAlias => false,
        }
    }

    pub(crate) fn is_function_like(self) -> bool {
        // Type parameter scopes behave like function scopes in terms of name resolution; CPython
        // symbol table also uses the term "function-like" for these scopes.
        matches!(
            self,
            ScopeKind::Annotation
                | ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::TypeAlias
                | ScopeKind::Comprehension
        )
    }

    pub(crate) fn is_class(self) -> bool {
        matches!(self, ScopeKind::Class)
    }

    pub(crate) fn is_type_parameter(self) -> bool {
        matches!(self, ScopeKind::Annotation | ScopeKind::TypeAlias)
    }
}

/// Symbol table for a specific [`Scope`].
#[derive(Default, salsa::Update)]
pub struct SymbolTable {
    /// The symbols in this scope.
    symbols: IndexVec<ScopedSymbolId, Symbol>,

    /// The symbols indexed by name.
    symbols_by_name: SymbolMap,
}

impl SymbolTable {
    fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
    }

    pub(crate) fn symbol(&self, symbol_id: impl Into<ScopedSymbolId>) -> &Symbol {
        &self.symbols[symbol_id.into()]
    }

    #[expect(unused)]
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

impl std::fmt::Debug for SymbolTable {
    /// Exclude the `symbols_by_name` field from the debug output.
    /// It's very noisy and not useful for debugging.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SymbolTable")
            .field(&self.symbols)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(super) struct SymbolTableBuilder {
    table: SymbolTable,
}

impl SymbolTableBuilder {
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

    pub(super) fn mark_symbol_declared(&mut self, id: ScopedSymbolId) {
        self.table.symbols[id].insert_flags(SymbolFlags::IS_DECLARED);
    }

    pub(super) fn mark_symbol_used(&mut self, id: ScopedSymbolId) {
        self.table.symbols[id].insert_flags(SymbolFlags::IS_USED);
    }

    pub(super) fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.table.symbols()
    }

    pub(super) fn symbol_id_by_name(&self, name: &str) -> Option<ScopedSymbolId> {
        self.table.symbol_id_by_name(name)
    }

    pub(super) fn symbol(&self, symbol_id: impl Into<ScopedSymbolId>) -> &Symbol {
        self.table.symbol(symbol_id)
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
    TypeAlias(&'a ast::StmtTypeAlias),
    TypeAliasTypeParameters(&'a ast::StmtTypeAlias),
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
    #[expect(unsafe_code)]
    pub(super) unsafe fn to_kind(self, module: ParsedModule) -> NodeWithScopeKind {
        unsafe {
            match self {
                NodeWithScopeRef::Module => NodeWithScopeKind::Module,
                NodeWithScopeRef::Class(class) => {
                    NodeWithScopeKind::Class(AstNodeRef::new(module, class))
                }
                NodeWithScopeRef::Function(function) => {
                    NodeWithScopeKind::Function(AstNodeRef::new(module, function))
                }
                NodeWithScopeRef::TypeAlias(type_alias) => {
                    NodeWithScopeKind::TypeAlias(AstNodeRef::new(module, type_alias))
                }
                NodeWithScopeRef::TypeAliasTypeParameters(type_alias) => {
                    NodeWithScopeKind::TypeAliasTypeParameters(AstNodeRef::new(module, type_alias))
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
            NodeWithScopeRef::TypeAlias(type_alias) => {
                NodeWithScopeKey::TypeAlias(NodeKey::from_node(type_alias))
            }
            NodeWithScopeRef::TypeAliasTypeParameters(type_alias) => {
                NodeWithScopeKey::TypeAliasTypeParameters(NodeKey::from_node(type_alias))
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
#[derive(Clone, Debug, salsa::Update)]
pub enum NodeWithScopeKind {
    Module,
    Class(AstNodeRef<ast::StmtClassDef>),
    ClassTypeParameters(AstNodeRef<ast::StmtClassDef>),
    Function(AstNodeRef<ast::StmtFunctionDef>),
    FunctionTypeParameters(AstNodeRef<ast::StmtFunctionDef>),
    TypeAliasTypeParameters(AstNodeRef<ast::StmtTypeAlias>),
    TypeAlias(AstNodeRef<ast::StmtTypeAlias>),
    Lambda(AstNodeRef<ast::ExprLambda>),
    ListComprehension(AstNodeRef<ast::ExprListComp>),
    SetComprehension(AstNodeRef<ast::ExprSetComp>),
    DictComprehension(AstNodeRef<ast::ExprDictComp>),
    GeneratorExpression(AstNodeRef<ast::ExprGenerator>),
}

impl NodeWithScopeKind {
    pub(crate) const fn scope_kind(&self) -> ScopeKind {
        match self {
            Self::Module => ScopeKind::Module,
            Self::Class(_) => ScopeKind::Class,
            Self::Function(_) => ScopeKind::Function,
            Self::Lambda(_) => ScopeKind::Lambda,
            Self::FunctionTypeParameters(_)
            | Self::ClassTypeParameters(_)
            | Self::TypeAliasTypeParameters(_) => ScopeKind::Annotation,
            Self::TypeAlias(_) => ScopeKind::TypeAlias,
            Self::ListComprehension(_)
            | Self::SetComprehension(_)
            | Self::DictComprehension(_)
            | Self::GeneratorExpression(_) => ScopeKind::Comprehension,
        }
    }

    pub fn expect_class(&self) -> &ast::StmtClassDef {
        match self {
            Self::Class(class) => class.node(),
            _ => panic!("expected class"),
        }
    }

    pub(crate) const fn as_class(&self) -> Option<&ast::StmtClassDef> {
        match self {
            Self::Class(class) => Some(class.node()),
            _ => None,
        }
    }

    pub fn expect_function(&self) -> &ast::StmtFunctionDef {
        self.as_function().expect("expected function")
    }

    pub fn expect_type_alias(&self) -> &ast::StmtTypeAlias {
        match self {
            Self::TypeAlias(type_alias) => type_alias.node(),
            _ => panic!("expected type alias"),
        }
    }

    pub const fn as_function(&self) -> Option<&ast::StmtFunctionDef> {
        match self {
            Self::Function(function) => Some(function.node()),
            _ => None,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) enum NodeWithScopeKey {
    Module,
    Class(NodeKey),
    ClassTypeParameters(NodeKey),
    Function(NodeKey),
    FunctionTypeParameters(NodeKey),
    TypeAlias(NodeKey),
    TypeAliasTypeParameters(NodeKey),
    Lambda(NodeKey),
    ListComprehension(NodeKey),
    SetComprehension(NodeKey),
    DictComprehension(NodeKey),
    GeneratorExpression(NodeKey),
}
