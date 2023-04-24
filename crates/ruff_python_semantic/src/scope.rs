use std::num::TryFromIntError;
use std::ops::{Deref, Index, IndexMut};

use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Arguments, Expr, Keyword, Stmt};

use crate::binding::{BindingId, StarImportation};

#[derive(Debug)]
pub struct Scope<'a> {
    pub id: ScopeId,
    pub kind: ScopeKind<'a>,
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
        Scope::local(ScopeId::global(), ScopeKind::Module)
    }

    pub fn local(id: ScopeId, kind: ScopeKind<'a>) -> Self {
        Scope {
            id,
            kind,
            uses_locals: false,
            star_imports: Vec::default(),
            bindings: FxHashMap::default(),
            shadowed_bindings: FxHashMap::default(),
        }
    }

    /// Returns the [id](BindingId) of the binding bound to the given name.
    pub fn get(&self, name: &str) -> Option<&BindingId> {
        self.bindings.get(name)
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
    pub fn binding_ids(&self) -> std::collections::hash_map::Values<&str, BindingId> {
        self.bindings.values()
    }

    /// Returns a tuple of the name and id of all bindings defined in this scope.
    pub fn bindings(&self) -> std::collections::hash_map::Iter<&'a str, BindingId> {
        self.bindings.iter()
    }

    /// Returns an iterator over all [bindings](BindingId) bound to the given name, including
    /// those that were shadowed by later bindings.
    pub fn bindings_for_name(&self, name: &str) -> impl Iterator<Item = &BindingId> {
        self.bindings
            .get(name)
            .into_iter()
            .chain(self.shadowed_bindings.get(name).into_iter().flatten().rev())
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

/// Id uniquely identifying a scope in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max `u32::max`
/// and it is impossible to have more scopes than characters in the file (because defining a function or class
/// requires more than one character).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ScopeId(u32);

impl ScopeId {
    /// Returns the ID for the global scope
    #[inline]
    pub const fn global() -> Self {
        ScopeId(0)
    }

    /// Returns `true` if this is the id of the global scope
    pub const fn is_global(&self) -> bool {
        self.0 == 0
    }
}

impl TryFrom<usize> for ScopeId {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl From<ScopeId> for usize {
    fn from(value: ScopeId) -> Self {
        value.0 as usize
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
    // Properties derived from StmtKind::FunctionDef.
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
    // Properties derived from StmtKind::ClassDef.
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

/// The scopes of a program indexed by [`ScopeId`]
#[derive(Debug)]
pub struct Scopes<'a>(Vec<Scope<'a>>);

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
    pub fn push_scope(&mut self, kind: ScopeKind<'a>) -> ScopeId {
        let next_id = ScopeId::try_from(self.0.len()).unwrap();
        self.0.push(Scope::local(next_id, kind));
        next_id
    }
}

impl Default for Scopes<'_> {
    fn default() -> Self {
        Self(vec![Scope::global()])
    }
}

impl<'a> Index<ScopeId> for Scopes<'a> {
    type Output = Scope<'a>;

    fn index(&self, index: ScopeId) -> &Self::Output {
        &self.0[usize::from(index)]
    }
}

impl<'a> IndexMut<ScopeId> for Scopes<'a> {
    fn index_mut(&mut self, index: ScopeId) -> &mut Self::Output {
        &mut self.0[usize::from(index)]
    }
}

impl<'a> Deref for Scopes<'a> {
    type Target = [Scope<'a>];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct ScopeStack(Vec<ScopeId>);

impl ScopeStack {
    /// Pushes a new scope on the stack
    pub fn push(&mut self, id: ScopeId) {
        self.0.push(id);
    }

    /// Pops the top most scope
    pub fn pop(&mut self) -> Option<ScopeId> {
        self.0.pop()
    }

    /// Returns the id of the top-most
    pub fn top(&self) -> Option<ScopeId> {
        self.0.last().copied()
    }

    /// Returns an iterator from the current scope to the top scope (reverse iterator)
    pub fn iter(&self) -> std::iter::Rev<std::slice::Iter<ScopeId>> {
        self.0.iter().rev()
    }

    pub fn snapshot(&self) -> ScopeStackSnapshot {
        ScopeStackSnapshot(self.0.len())
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn restore(&mut self, snapshot: ScopeStackSnapshot) {
        self.0.truncate(snapshot.0);
    }
}

pub struct ScopeStackSnapshot(usize);

impl Default for ScopeStack {
    fn default() -> Self {
        Self(vec![ScopeId::global()])
    }
}
