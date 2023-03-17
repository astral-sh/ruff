use std::num::TryFromIntError;
use std::ops::Deref;

use rustc_hash::FxHashMap;
use rustpython_parser::ast::{Arguments, Expr, Keyword, Located, Location, Stmt};

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub location: Location,
    pub end_location: Location,
}

impl Range {
    pub const fn new(location: Location, end_location: Location) -> Self {
        Self {
            location,
            end_location,
        }
    }
}

impl<T> From<&Located<T>> for Range {
    fn from(located: &Located<T>) -> Self {
        Range::new(located.location, located.end_location.unwrap())
    }
}

impl<T> From<&Box<Located<T>>> for Range {
    fn from(located: &Box<Located<T>>) -> Self {
        Range::new(located.location, located.end_location.unwrap())
    }
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

#[derive(Debug)]
pub enum ScopeKind<'a> {
    Class(ClassDef<'a>),
    Function(FunctionDef<'a>),
    Generator,
    Module,
    Lambda(Lambda<'a>),
}

/// Id uniquely identifying a scope in a program.
///
/// Using a `u32` is sufficient because Ruff only supports parsing documents with a size of max `u32::max`:
/// A new scope requires a statement with a block body (and the right indention). That means, the upper bound of
/// scopes is defined by `u32::max / 8` (`if 1:\n x`)
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
pub struct Scope<'a> {
    pub id: ScopeId,
    pub kind: ScopeKind<'a>,
    pub import_starred: bool,
    pub uses_locals: bool,
    /// A map from bound name to binding index, for live bindings.
    pub bindings: FxHashMap<&'a str, BindingId>,
    /// A map from bound name to binding index, for bindings that were created
    /// in the scope but rebound (and thus overridden) later on in the same
    /// scope.
    pub rebounds: FxHashMap<&'a str, Vec<BindingId>>,
}

impl<'a> Scope<'a> {
    pub fn new(id: ScopeId, kind: ScopeKind<'a>) -> Self {
        Scope {
            id,
            kind,
            import_starred: false,
            uses_locals: false,
            bindings: FxHashMap::default(),
            rebounds: FxHashMap::default(),
        }
    }
}

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
}

#[derive(Copy, Debug, Clone)]
pub enum ExecutionContext {
    Runtime,
    Typing,
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

#[derive(Debug, Copy, Clone)]
pub struct RefEquality<'a, T>(pub &'a T);

impl<'a, T> std::hash::Hash for RefEquality<'a, T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        (self.0 as *const T).hash(state);
    }
}

impl<'a, 'b, T> PartialEq<RefEquality<'b, T>> for RefEquality<'a, T> {
    fn eq(&self, other: &RefEquality<'b, T>) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for RefEquality<'a, T> {}

impl<'a, T> Deref for RefEquality<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a> From<&RefEquality<'a, Stmt>> for &'a Stmt {
    fn from(r: &RefEquality<'a, Stmt>) -> Self {
        r.0
    }
}

impl<'a> From<&RefEquality<'a, Expr>> for &'a Expr {
    fn from(r: &RefEquality<'a, Expr>) -> Self {
        r.0
    }
}

pub type CallPath<'a> = smallvec::SmallVec<[&'a str; 8]>;
