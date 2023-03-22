use crate::types::{Range, RefEquality};
use bitflags::bitflags;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Arguments, Expr, Keyword, Stmt};
use std::num::TryFromIntError;
use std::ops::{Deref, Index, IndexMut};

#[derive(Debug)]
pub struct Scope<'a> {
    pub id: ScopeId,
    pub kind: ScopeKind<'a>,
    pub import_starred: bool,
    pub uses_locals: bool,
    /// A map from bound name to binding index, for live bindings.
    bindings: FxHashMap<&'a str, BindingId>,
    /// A map from bound name to binding index, for bindings that were created
    /// in the scope but rebound (and thus overridden) later on in the same
    /// scope.
    pub rebounds: FxHashMap<&'a str, Vec<BindingId>>,
}

impl<'a> Scope<'a> {
    pub fn global() -> Self {
        Scope::local(ScopeId::global(), ScopeKind::Module)
    }

    pub fn local(id: ScopeId, kind: ScopeKind<'a>) -> Self {
        Scope {
            id,
            kind,
            import_starred: false,
            uses_locals: false,
            bindings: FxHashMap::default(),
            rebounds: FxHashMap::default(),
        }
    }

    /// Returns the [id](BindingId) of the binding with the given name.
    pub fn get(&self, name: &str) -> Option<&BindingId> {
        self.bindings.get(name)
    }

    /// Adds a new binding with the given name to this scope.
    pub fn add(&mut self, name: &'a str, id: BindingId) -> Option<BindingId> {
        self.bindings.insert(name, id)
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

    pub fn bindings(&self) -> std::collections::hash_map::Iter<&'a str, BindingId> {
        self.bindings.iter()
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

#[derive(Debug)]
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
    pub(crate) fn push_scope(&mut self, kind: ScopeKind<'a>) -> ScopeId {
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
}

impl Default for ScopeStack {
    fn default() -> Self {
        Self(vec![ScopeId::global()])
    }
}

bitflags! {
    pub struct Exceptions: u32 {
        const NAME_ERROR = 0b0000_0001;
        const MODULE_NOT_FOUND_ERROR = 0b0000_0010;
        const IMPORT_ERROR = 0b0000_0100;
    }
}

#[derive(Debug, Clone)]
pub struct Binding<'a> {
    pub kind: BindingKind<'a>,
    pub range: Range,
    /// The context in which the binding was created.
    pub context: ExecutionContext,
    /// The statement in which the [`Binding`] was defined.
    pub source: Option<RefEquality<'a, Stmt>>,
    /// Tuple of (scope index, range) indicating the scope and range at which
    /// the binding was last used in a runtime context.
    pub runtime_usage: Option<(ScopeId, Range)>,
    /// Tuple of (scope index, range) indicating the scope and range at which
    /// the binding was last used in a typing-time context.
    pub typing_usage: Option<(ScopeId, Range)>,
    /// Tuple of (scope index, range) indicating the scope and range at which
    /// the binding was last used in a synthetic context. This is used for
    /// (e.g.) `__future__` imports, explicit re-exports, and other bindings
    /// that should be considered used even if they're never referenced.
    pub synthetic_usage: Option<(ScopeId, Range)>,
    /// The exceptions that were handled when the binding was defined.
    pub exceptions: Exceptions,
}

impl<'a> Binding<'a> {
    pub fn mark_used(&mut self, scope: ScopeId, range: Range, context: ExecutionContext) {
        match context {
            ExecutionContext::Runtime => self.runtime_usage = Some((scope, range)),
            ExecutionContext::Typing => self.typing_usage = Some((scope, range)),
        }
    }

    pub const fn used(&self) -> bool {
        self.runtime_usage.is_some()
            || self.synthetic_usage.is_some()
            || self.typing_usage.is_some()
    }

    pub const fn is_definition(&self) -> bool {
        matches!(
            self.kind,
            BindingKind::ClassDefinition
                | BindingKind::FunctionDefinition
                | BindingKind::Builtin
                | BindingKind::FutureImportation
                | BindingKind::StarImportation(..)
                | BindingKind::Importation(..)
                | BindingKind::FromImportation(..)
                | BindingKind::SubmoduleImportation(..)
        )
    }

    pub fn redefines(&self, existing: &'a Binding) -> bool {
        match &self.kind {
            BindingKind::Importation(.., full_name) => {
                if let BindingKind::SubmoduleImportation(.., existing) = &existing.kind {
                    return full_name == existing;
                }
            }
            BindingKind::FromImportation(.., full_name) => {
                if let BindingKind::SubmoduleImportation(.., existing) = &existing.kind {
                    return full_name == existing;
                }
            }
            BindingKind::SubmoduleImportation(.., full_name) => match &existing.kind {
                BindingKind::Importation(.., existing)
                | BindingKind::SubmoduleImportation(.., existing) => {
                    return full_name == existing;
                }
                BindingKind::FromImportation(.., existing) => {
                    return full_name == existing;
                }
                _ => {}
            },
            BindingKind::Annotation => {
                return false;
            }
            BindingKind::FutureImportation => {
                return false;
            }
            BindingKind::StarImportation(..) => {
                return false;
            }
            _ => {}
        }
        existing.is_definition()
    }
}

#[derive(Copy, Debug, Clone)]
pub enum ExecutionContext {
    Runtime,
    Typing,
}

/// ID uniquely identifying a [Binding] in a program.
///
/// Using a `u32` to identify [Binding]s should is sufficient because Ruff only supports documents with a
/// size smaller than or equal to `u32::max`. A document with the size of `u32::max` must have fewer than `u32::max`
/// bindings because bindings must be separated by whitespace (and have an assignment).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingId(u32);

impl From<BindingId> for usize {
    fn from(value: BindingId) -> Self {
        value.0 as usize
    }
}

impl TryFrom<usize> for BindingId {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl nohash_hasher::IsEnabled for BindingId {}

// Pyflakes defines the following binding hierarchy (via inheritance):
//   Binding
//    ExportBinding
//    Annotation
//    Argument
//    Assignment
//      NamedExprAssignment
//    Definition
//      FunctionDefinition
//      ClassDefinition
//      Builtin
//      Importation
//        SubmoduleImportation
//        ImportationFrom
//        StarImportation
//        FutureImportation

#[derive(Clone, Debug, is_macro::Is)]
pub enum BindingKind<'a> {
    Annotation,
    Argument,
    Assignment,
    Binding,
    LoopVar,
    Global,
    Nonlocal,
    Builtin,
    ClassDefinition,
    FunctionDefinition,
    Export(Vec<String>),
    FutureImportation,
    StarImportation(Option<usize>, Option<String>),
    Importation(&'a str, &'a str),
    FromImportation(&'a str, String),
    SubmoduleImportation(&'a str, &'a str),
}

/// The bindings in a program.
///
/// Bindings are indexed by [`BindingId`]
#[derive(Debug, Clone, Default)]
pub struct Bindings<'a>(Vec<Binding<'a>>);

impl<'a> Bindings<'a> {
    /// Pushes a new binding and returns its id
    pub fn push(&mut self, binding: Binding<'a>) -> BindingId {
        let id = self.next_id();
        self.0.push(binding);
        id
    }

    /// Returns the id that will be assigned when pushing the next binding
    pub fn next_id(&self) -> BindingId {
        BindingId::try_from(self.0.len()).unwrap()
    }
}

impl<'a> Index<BindingId> for Bindings<'a> {
    type Output = Binding<'a>;

    fn index(&self, index: BindingId) -> &Self::Output {
        &self.0[usize::from(index)]
    }
}

impl<'a> IndexMut<BindingId> for Bindings<'a> {
    fn index_mut(&mut self, index: BindingId) -> &mut Self::Output {
        &mut self.0[usize::from(index)]
    }
}

impl<'a> Deref for Bindings<'a> {
    type Target = [Binding<'a>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> FromIterator<Binding<'a>> for Bindings<'a> {
    fn from_iter<T: IntoIterator<Item = Binding<'a>>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}
