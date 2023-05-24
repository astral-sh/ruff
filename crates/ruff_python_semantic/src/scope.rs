use std::ops::{Deref, DerefMut};

use ruff_index::{newtype_index, Idx, IndexSlice, IndexVec};
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Arguments, Expr, Keyword, Stmt};

use crate::binding::{BindingId, StarImportation};

#[derive(Debug)]
pub struct Scope<'a> {
    pub kind: ScopeKind<'a>,
    pub parent: Option<ScopeId>,
    /// Whether this scope uses the `locals()` builtin.
    pub uses_locals: bool,
    /// A list of star imports in this scope. These represent _module_ imports (e.g., `sys` in
    /// `from sys import *`), rather than individual bindings (e.g., individual members in `sys`).
    star_imports: Vec<StarImportation<'a>>,
    /// A map from bound name to binding index, for current bindings.
    bindings: FxHashMap<&'a str, BindingId>,
    /// A map from bound name to binding index, for bindings that were shadowed later in the scope.
    shadowed_bindings: FxHashMap<&'a str, Vec<BindingId>>,
}

impl<'a> Scope<'a> {
    pub fn global() -> Self {
        Scope {
            kind: ScopeKind::Module,
            parent: None,
            uses_locals: false,
            star_imports: Vec::default(),
            bindings: FxHashMap::default(),
            shadowed_bindings: FxHashMap::default(),
        }
    }

    pub fn local(kind: ScopeKind<'a>, parent: ScopeId) -> Self {
        Scope {
            kind,
            parent: Some(parent),
            uses_locals: false,
            star_imports: Vec::default(),
            bindings: FxHashMap::default(),
            shadowed_bindings: FxHashMap::default(),
        }
    }

    /// Returns the [id](BindingId) of the binding bound to the given name.
    pub fn get(&self, name: &str) -> Option<BindingId> {
        self.bindings.get(name).copied()
    }

    /// Adds a new binding with the given name to this scope.
    pub fn add(&mut self, name: &'a str, id: BindingId) -> Option<BindingId> {
        if let Some(id) = self.bindings.insert(name, id) {
            self.shadowed_bindings.entry(name).or_default().push(id);
            Some(id)
        } else {
            None
        }
    }

    /// Returns `true` if this scope defines a binding with the given name.
    pub fn defines(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Removes the binding with the given name
    pub fn remove(&mut self, name: &str) -> Option<BindingId> {
        self.bindings.remove(name)
    }

    /// Returns the ids of all bindings defined in this scope.
    pub fn binding_ids(&self) -> impl Iterator<Item = BindingId> + '_ {
        self.bindings.values().copied()
    }

    /// Returns a tuple of the name and id of all bindings defined in this scope.
    pub fn bindings(&self) -> impl Iterator<Item = (&str, BindingId)> + '_ {
        self.bindings.iter().map(|(&name, &id)| (name, id))
    }

    /// Returns an iterator over all [bindings](BindingId) bound to the given name, including
    /// those that were shadowed by later bindings.
    pub fn bindings_for_name(&self, name: &str) -> impl Iterator<Item = BindingId> + '_ {
        self.bindings
            .get(name)
            .into_iter()
            .chain(self.shadowed_bindings.get(name).into_iter().flatten().rev())
            .copied()
    }

    /// Adds a reference to a star import (e.g., `from sys import *`) to this scope.
    pub fn add_star_import(&mut self, import: StarImportation<'a>) {
        self.star_imports.push(import);
    }

    /// Returns `true` if this scope contains a star import (e.g., `from sys import *`).
    pub fn uses_star_imports(&self) -> bool {
        !self.star_imports.is_empty()
    }

    /// Returns an iterator over all star imports (e.g., `from sys import *`) in this scope.
    pub fn star_imports(&self) -> impl Iterator<Item = &StarImportation<'a>> {
        self.star_imports.iter()
    }
}

#[derive(Debug, is_macro::Is)]
pub enum ScopeKind<'a> {
    Class(ClassDef<'a>),
    Function(FunctionDef<'a>),
    Generator,
    Module,
    Lambda(Lambda<'a>),
}

#[derive(Debug)]
pub struct FunctionDef<'a> {
    // Properties derived from Stmt::FunctionDef.
    pub name: &'a str,
    pub args: &'a Arguments,
    pub body: &'a [Stmt],
    pub decorator_list: &'a [Expr],
    // pub returns: Option<&'a Expr>,
    // pub type_comment: Option<&'a str>,
    // Scope-specific properties.
    // TODO(charlie): Create AsyncFunctionDef to mirror the AST.
    pub async_: bool,
    pub globals: FxHashMap<&'a str, &'a Stmt>,
}

#[derive(Debug)]
pub struct ClassDef<'a> {
    // Properties derived from Stmt::ClassDef.
    pub name: &'a str,
    pub bases: &'a [Expr],
    pub keywords: &'a [Keyword],
    // pub body: &'a [Stmt],
    pub decorator_list: &'a [Expr],
    // Scope-specific properties.
    pub globals: FxHashMap<&'a str, &'a Stmt>,
}

#[derive(Debug)]
pub struct Lambda<'a> {
    pub args: &'a Arguments,
    pub body: &'a Expr,
}

/// Id uniquely identifying a scope in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max `u32::max`
/// and it is impossible to have more scopes than characters in the file (because defining a function or class
/// requires more than one character).
#[newtype_index]
pub struct ScopeId;

impl ScopeId {
    /// Returns the ID for the global scope
    #[inline]
    pub const fn global() -> Self {
        ScopeId::from_u32(0)
    }

    /// Returns `true` if this is the id of the global scope
    pub const fn is_global(&self) -> bool {
        self.index() == 0
    }
}

/// The scopes of a program indexed by [`ScopeId`]
#[derive(Debug)]
pub struct Scopes<'a>(IndexVec<ScopeId, Scope<'a>>);

impl<'a> Scopes<'a> {
    /// Returns a reference to the global scope
    pub fn global(&self) -> &Scope<'a> {
        &self[ScopeId::global()]
    }

    /// Returns a mutable reference to the global scope
    pub fn global_mut(&mut self) -> &mut Scope<'a> {
        &mut self[ScopeId::global()]
    }

    /// Pushes a new scope and returns its unique id
    pub fn push_scope(&mut self, kind: ScopeKind<'a>, parent: ScopeId) -> ScopeId {
        let next_id = ScopeId::new(self.0.len());
        self.0.push(Scope::local(kind, parent));
        next_id
    }

    /// Returns an iterator over all [`ScopeId`] ancestors, starting from the given [`ScopeId`].
    pub fn ancestor_ids(&self, scope_id: ScopeId) -> impl Iterator<Item = ScopeId> + '_ {
        std::iter::successors(Some(scope_id), |&scope_id| self[scope_id].parent)
    }

    /// Returns an iterator over all [`Scope`] ancestors, starting from the given [`ScopeId`].
    pub fn ancestors(&self, scope_id: ScopeId) -> impl Iterator<Item = &Scope> + '_ {
        std::iter::successors(Some(&self[scope_id]), |&scope| {
            scope.parent.map(|scope_id| &self[scope_id])
        })
    }
}

impl Default for Scopes<'_> {
    fn default() -> Self {
        Self(IndexVec::from_raw(vec![Scope::global()]))
    }
}

impl<'a> Deref for Scopes<'a> {
    type Target = IndexSlice<ScopeId, Scope<'a>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for Scopes<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
