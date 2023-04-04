use std::num::TryFromIntError;
use std::ops::{Deref, Index, IndexMut};

use bitflags::bitflags;
use rustpython_parser::ast::Stmt;

use ruff_python_ast::types::{Range, RefEquality};

use crate::scope::ScopeId;

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
                | BindingKind::Importation(..)
                | BindingKind::FromImportation(..)
                | BindingKind::SubmoduleImportation(..)
        )
    }

    pub fn redefines(&self, existing: &'a Binding) -> bool {
        match &self.kind {
            BindingKind::Importation(Importation { full_name, .. }) => {
                if let BindingKind::SubmoduleImportation(SubmoduleImportation {
                    full_name: existing,
                    ..
                }) = &existing.kind
                {
                    return full_name == existing;
                }
            }
            BindingKind::FromImportation(FromImportation { full_name, .. }) => {
                if let BindingKind::SubmoduleImportation(SubmoduleImportation {
                    full_name: existing,
                    ..
                }) = &existing.kind
                {
                    return full_name == existing;
                }
            }
            BindingKind::SubmoduleImportation(SubmoduleImportation { full_name, .. }) => {
                match &existing.kind {
                    BindingKind::Importation(Importation {
                        full_name: existing,
                        ..
                    })
                    | BindingKind::SubmoduleImportation(SubmoduleImportation {
                        full_name: existing,
                        ..
                    }) => {
                        return full_name == existing;
                    }
                    BindingKind::FromImportation(FromImportation {
                        full_name: existing,
                        ..
                    }) => {
                        return full_name == existing;
                    }
                    _ => {}
                }
            }
            BindingKind::Annotation => {
                return false;
            }
            BindingKind::FutureImportation => {
                return false;
            }
            _ => {}
        }
        existing.is_definition()
    }
}

/// ID uniquely identifying a [Binding] in a program.
///
/// Using a `u32` to identify [Binding]s should is sufficient because Ruff only supports documents with a
/// size smaller than or equal to `u32::max`. A document with the size of `u32::max` must have fewer than `u32::max`
/// bindings because bindings must be separated by whitespace (and have an assignment).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingId(u32);

impl TryFrom<usize> for BindingId {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(value)?))
    }
}

impl nohash_hasher::IsEnabled for BindingId {}

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

impl From<BindingId> for usize {
    fn from(value: BindingId) -> Self {
        value.0 as usize
    }
}

#[derive(Debug, Clone)]
pub struct StarImportation<'a> {
    /// The level of the import. `None` or `Some(0)` indicate an absolute import.
    pub level: Option<usize>,
    /// The module being imported. `None` indicates a wildcard import.
    pub module: Option<&'a str>,
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
//        FutureImportation

#[derive(Clone, Debug)]
pub struct Export<'a> {
    /// The names of the bindings exported via `__all__`.
    pub names: Vec<&'a str>,
}

#[derive(Clone, Debug)]
pub struct Importation<'a> {
    /// The name to which the import is bound.
    /// Given `import foo`, `name` would be "foo".
    /// Given `import foo as bar`, `name` would be "bar".
    pub name: &'a str,
    /// The full name of the module being imported.
    /// Given `import foo`, `full_name` would be "foo".
    /// Given `import foo as bar`, `full_name` would be "foo".
    pub full_name: &'a str,
}

#[derive(Clone, Debug)]
pub struct FromImportation<'a> {
    /// The name to which the import is bound.
    /// Given `from foo import bar`, `name` would be "bar".
    /// Given `from foo import bar as baz`, `name` would be "baz".
    pub name: &'a str,
    /// The full name of the module being imported.
    /// Given `from foo import bar`, `full_name` would be "foo.bar".
    /// Given `from foo import bar as baz`, `full_name` would be "foo.bar".
    pub full_name: String,
}

#[derive(Clone, Debug)]
pub struct SubmoduleImportation<'a> {
    /// The parent module imported by the submodule import.
    /// Given `import foo.bar`, `module` would be "foo".
    pub name: &'a str,
    /// The full name of the submodule being imported.
    /// Given `import foo.bar`, `full_name` would be "foo.bar".
    pub full_name: &'a str,
}

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
    Export(Export<'a>),
    FutureImportation,
    Importation(Importation<'a>),
    FromImportation(FromImportation<'a>),
    SubmoduleImportation(SubmoduleImportation<'a>),
}

bitflags! {
    pub struct Exceptions: u32 {
        const NAME_ERROR = 0b0000_0001;
        const MODULE_NOT_FOUND_ERROR = 0b0000_0010;
        const IMPORT_ERROR = 0b0000_0100;
    }
}

#[derive(Copy, Debug, Clone)]
pub enum ExecutionContext {
    Runtime,
    Typing,
}
