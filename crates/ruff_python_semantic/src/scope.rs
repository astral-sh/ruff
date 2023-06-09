use bitflags::bitflags;
use nohash_hasher::{BuildNoHashHasher, IntMap};
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use rustc_hash::FxHashMap;
use rustpython_parser::ast;

use ruff_index::{newtype_index, Idx, IndexSlice, IndexVec};

use crate::binding::{BindingId, StarImportation};
use crate::globals::GlobalsId;

#[derive(Debug)]
pub struct Scope<'a> {
    pub kind: ScopeKind<'a>,
    pub parent: Option<ScopeId>,
    /// A list of star imports in this scope. These represent _module_ imports (e.g., `sys` in
    /// `from sys import *`), rather than individual bindings (e.g., individual members in `sys`).
    star_imports: Vec<StarImportation<'a>>,
    /// A map from bound name to binding ID.
    bindings: FxHashMap<&'a str, BindingId>,
    /// A map from binding ID to binding ID that it shadows.
    shadowed_bindings: HashMap<BindingId, BindingId, BuildNoHashHasher<BindingId>>,
    /// A list of all names that have been deleted in this scope.
    deleted_symbols: Vec<&'a str>,
    /// Index into the globals arena, if the scope contains any globally-declared symbols.
    globals_id: Option<GlobalsId>,
    /// Flags for the [`Scope`].
    flags: ScopeFlags,
}

impl<'a> Scope<'a> {
    pub fn global() -> Self {
        Scope {
            kind: ScopeKind::Module,
            parent: None,
            star_imports: Vec::default(),
            bindings: FxHashMap::default(),
            shadowed_bindings: IntMap::default(),
            deleted_symbols: Vec::default(),
            globals_id: None,
            flags: ScopeFlags::empty(),
        }
    }

    pub fn local(kind: ScopeKind<'a>, parent: ScopeId) -> Self {
        Scope {
            kind,
            parent: Some(parent),
            star_imports: Vec::default(),
            bindings: FxHashMap::default(),
            shadowed_bindings: IntMap::default(),
            deleted_symbols: Vec::default(),
            globals_id: None,
            flags: ScopeFlags::empty(),
        }
    }

    /// Returns the [id](BindingId) of the binding bound to the given name.
    pub fn get(&self, name: &str) -> Option<BindingId> {
        self.bindings.get(name).copied()
    }

    /// Adds a new binding with the given name to this scope.
    pub fn add(&mut self, name: &'a str, id: BindingId) -> Option<BindingId> {
        if let Some(shadowed) = self.bindings.insert(name, id) {
            self.shadowed_bindings.insert(id, shadowed);
            Some(shadowed)
        } else {
            None
        }
    }

    /// Removes the binding with the given name.
    pub fn delete(&mut self, name: &'a str) -> Option<BindingId> {
        self.deleted_symbols.push(name);
        self.bindings.remove(name)
    }

    /// Returns `true` if this scope has a binding with the given name.
    pub fn has(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Returns `true` if the scope declares a symbol with the given name.
    ///
    /// Unlike [`Scope::has`], the name may no longer be bound to a value (e.g., it could be
    /// deleted).
    pub fn declares(&self, name: &str) -> bool {
        self.has(name) || self.deleted_symbols.contains(&name)
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
        std::iter::successors(self.bindings.get(name).copied(), |id| {
            self.shadowed_bindings.get(id).copied()
        })
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

    /// Set the globals pointer for this scope.
    pub fn set_globals_id(&mut self, globals: GlobalsId) {
        self.globals_id = Some(globals);
    }

    /// Returns the globals pointer for this scope.
    pub fn globals_id(&self) -> Option<GlobalsId> {
        self.globals_id
    }

    /// Sets the [`ScopeFlags::USES_LOCALS`] flag.
    pub fn set_uses_locals(&mut self) {
        self.flags.insert(ScopeFlags::USES_LOCALS);
    }

    /// Returns `true` if this scope uses locals (e.g., `locals()`).
    pub const fn uses_locals(&self) -> bool {
        self.flags.contains(ScopeFlags::USES_LOCALS)
    }
}

bitflags! {
    /// Flags on a [`Scope`].
    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct ScopeFlags: u8 {
        /// The scope uses locals (e.g., `locals()`).
        const USES_LOCALS = 1 << 0;
    }
}

#[derive(Debug, is_macro::Is)]
pub enum ScopeKind<'a> {
    Class(&'a ast::StmtClassDef),
    Function(&'a ast::StmtFunctionDef),
    AsyncFunction(&'a ast::StmtAsyncFunctionDef),
    Generator,
    Module,
    Lambda(&'a ast::ExprLambda),
}

impl ScopeKind<'_> {
    pub const fn is_any_function(&self) -> bool {
        matches!(self, ScopeKind::Function(_) | ScopeKind::AsyncFunction(_))
    }
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
