use std::ops::{Deref, DerefMut};

use bitflags::bitflags;
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, Idx, IndexSlice, IndexVec};

use crate::binding::BindingId;
use crate::globals::GlobalsId;
use crate::star_import::StarImport;

#[derive(Debug)]
pub struct Scope<'a> {
    /// The kind of scope.
    pub kind: ScopeKind<'a>,

    /// The parent scope, if any.
    pub parent: Option<ScopeId>,

    /// A list of star imports in this scope. These represent _module_ imports (e.g., `sys` in
    /// `from sys import *`), rather than individual bindings (e.g., individual members in `sys`).
    star_imports: Vec<StarImport<'a>>,

    /// A map from bound name to binding ID.
    bindings: FxHashMap<&'a str, BindingId>,

    /// A map from binding ID to binding ID that it shadows.
    ///
    /// For example:
    /// ```python
    /// def f():
    ///     x = 1
    ///     x = 2
    /// ```
    ///
    /// In this case, the binding created by `x = 2` shadows the binding created by `x = 1`.
    shadowed_bindings: FxHashMap<BindingId, BindingId>,

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
            shadowed_bindings: FxHashMap::default(),
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
            shadowed_bindings: FxHashMap::default(),
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

    /// Returns `true` if this scope has a binding with the given name.
    pub fn has(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Returns the IDs of all bindings defined in this scope.
    pub fn binding_ids(&self) -> impl Iterator<Item = BindingId> + '_ {
        self.bindings.values().copied()
    }

    /// Returns a tuple of the name and ID of all bindings defined in this scope.
    pub fn bindings(&self) -> impl Iterator<Item = (&'a str, BindingId)> + '_ {
        self.bindings.iter().map(|(&name, &id)| (name, id))
    }

    /// Like [`Scope::get`], but returns all bindings with the given name, including
    /// those that were shadowed by later bindings.
    pub fn get_all(&self, name: &str) -> impl Iterator<Item = BindingId> + '_ {
        std::iter::successors(self.bindings.get(name).copied(), |id| {
            self.shadowed_bindings.get(id).copied()
        })
    }

    /// Like [`Scope::bindings`], but returns all bindings added to the scope, including those that
    /// were shadowed by later bindings.
    pub fn all_bindings(&self) -> impl Iterator<Item = (&str, BindingId)> + '_ {
        self.bindings.iter().flat_map(|(&name, &id)| {
            std::iter::successors(Some(id), |id| self.shadowed_bindings.get(id).copied())
                .map(move |id| (name, id))
        })
    }

    /// Returns the ID of the binding that the given binding shadows, if any.
    pub fn shadowed_binding(&self, id: BindingId) -> Option<BindingId> {
        self.shadowed_bindings.get(&id).copied()
    }

    /// Returns an iterator over all bindings that the given binding shadows, including itself.
    pub fn shadowed_bindings(&self, id: BindingId) -> impl Iterator<Item = BindingId> + '_ {
        std::iter::successors(Some(id), |id| self.shadowed_bindings.get(id).copied())
    }

    /// Adds a reference to a star import (e.g., `from sys import *`) to this scope.
    pub fn add_star_import(&mut self, import: StarImport<'a>) {
        self.star_imports.push(import);
    }

    /// Returns `true` if this scope contains a star import (e.g., `from sys import *`).
    pub fn uses_star_imports(&self) -> bool {
        !self.star_imports.is_empty()
    }

    /// Set the globals pointer for this scope.
    pub(crate) fn set_globals_id(&mut self, globals: GlobalsId) {
        self.globals_id = Some(globals);
    }

    /// Returns the globals pointer for this scope.
    pub(crate) fn globals_id(&self) -> Option<GlobalsId> {
        self.globals_id
    }

    /// Sets the [`ScopeFlags::USES_LOCALS`] flag.
    pub fn set_uses_locals(&mut self) {
        self.flags.insert(ScopeFlags::USES_LOCALS);
    }

    /// Returns `true` if this scope uses locals (e.g., `locals()`).
    pub const fn uses_locals(&self) -> bool {
        self.flags.intersects(ScopeFlags::USES_LOCALS)
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
    Generator(GeneratorKind),
    Module,
    /// A Python 3.12+ [annotation scope](https://docs.python.org/3/reference/executionmodel.html#annotation-scopes)
    Type,
    Lambda(&'a ast::ExprLambda),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratorKind {
    Generator,
    ListComprehension,
    DictComprehension,
    SetComprehension,
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
    pub(crate) fn global(&self) -> &Scope<'a> {
        &self[ScopeId::global()]
    }

    /// Returns a mutable reference to the global scope
    pub(crate) fn global_mut(&mut self) -> &mut Scope<'a> {
        &mut self[ScopeId::global()]
    }

    /// Pushes a new scope and returns its unique id
    pub(crate) fn push_scope(&mut self, kind: ScopeKind<'a>, parent: ScopeId) -> ScopeId {
        let next_id = ScopeId::new(self.0.len());
        self.0.push(Scope::local(kind, parent));
        next_id
    }

    /// Returns an iterator over all [`ScopeId`] ancestors, starting from the given [`ScopeId`].
    pub fn ancestor_ids(&self, scope_id: ScopeId) -> impl Iterator<Item = ScopeId> + '_ {
        std::iter::successors(Some(scope_id), |&scope_id| self[scope_id].parent)
    }

    /// Returns an iterator over all [`Scope`] ancestors, starting from the given [`ScopeId`].
    pub fn ancestors(&self, scope_id: ScopeId) -> impl Iterator<Item = &Scope<'a>> + '_ {
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

impl DerefMut for Scopes<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
