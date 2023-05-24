use std::ops::{Deref, DerefMut};

use bitflags::bitflags;
use ruff_text_size::TextRange;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_python_ast::helpers;
use ruff_python_ast::source_code::Locator;

use crate::context::ExecutionContext;
use crate::model::SemanticModel;
use crate::node::NodeId;
use crate::reference::ReferenceId;

#[derive(Debug, Clone)]
pub struct Binding<'a> {
    pub kind: BindingKind<'a>,
    pub range: TextRange,
    /// The context in which the [`Binding`] was created.
    pub context: ExecutionContext,
    /// The statement in which the [`Binding`] was defined.
    pub source: Option<NodeId>,
    /// The references to the [`Binding`].
    pub references: Vec<ReferenceId>,
    /// The exceptions that were handled when the [`Binding`] was defined.
    pub exceptions: Exceptions,
    /// Flags for the [`Binding`].
    pub flags: BindingFlags,
}

impl<'a> Binding<'a> {
    /// Return `true` if this [`Binding`] is used.
    pub fn is_used(&self) -> bool {
        !self.references.is_empty()
    }

    /// Returns an iterator over all references for the current [`Binding`].
    pub fn references(&self) -> impl Iterator<Item = ReferenceId> + '_ {
        self.references.iter().copied()
    }

    /// Return `true` if this [`Binding`] represents an explicit re-export
    /// (e.g., `import FastAPI as FastAPI`).
    pub const fn is_explicit_export(&self) -> bool {
        self.flags.contains(BindingFlags::EXPLICIT_EXPORT)
    }

    /// Return `true` if this binding redefines the given binding.
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
        matches!(
            existing.kind,
            BindingKind::ClassDefinition
                | BindingKind::FunctionDefinition
                | BindingKind::Builtin
                | BindingKind::Importation(..)
                | BindingKind::FromImportation(..)
                | BindingKind::SubmoduleImportation(..)
        )
    }

    /// Returns the appropriate visual range for highlighting this binding.
    pub fn trimmed_range(&self, semantic_model: &SemanticModel, locator: &Locator) -> TextRange {
        match self.kind {
            BindingKind::ClassDefinition | BindingKind::FunctionDefinition => {
                self.source.map_or(self.range, |source| {
                    helpers::identifier_range(semantic_model.stmts[source], locator)
                })
            }
            _ => self.range,
        }
    }
}

bitflags! {
    /// Flags on a [`Binding`].
    #[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
    pub struct BindingFlags: u8 {
        /// The binding represents an explicit re-export.
        ///
        /// For example, the binding could be `FastAPI` in:
        /// ```python
        /// import FastAPI as FastAPI
        /// ```
        const EXPLICIT_EXPORT = 1 << 0;
    }
}

/// ID uniquely identifying a [Binding] in a program.
///
/// Using a `u32` to identify [Binding]s should is sufficient because Ruff only supports documents with a
/// size smaller than or equal to `u32::max`. A document with the size of `u32::max` must have fewer than `u32::max`
/// bindings because bindings must be separated by whitespace (and have an assignment).
#[newtype_index]
pub struct BindingId;

impl nohash_hasher::IsEnabled for BindingId {}

/// The bindings in a program.
///
/// Bindings are indexed by [`BindingId`]
#[derive(Debug, Clone, Default)]
pub struct Bindings<'a>(IndexVec<BindingId, Binding<'a>>);

impl<'a> Bindings<'a> {
    /// Pushes a new binding and returns its id
    pub fn push(&mut self, binding: Binding<'a>) -> BindingId {
        self.0.push(binding)
    }

    /// Returns the id that will be assigned when pushing the next binding
    pub fn next_id(&self) -> BindingId {
        self.0.next_index()
    }
}

impl<'a> Deref for Bindings<'a> {
    type Target = IndexSlice<BindingId, Binding<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for Bindings<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> FromIterator<Binding<'a>> for Bindings<'a> {
    fn from_iter<T: IntoIterator<Item = Binding<'a>>>(iter: T) -> Self {
        Self(IndexVec::from_iter(iter))
    }
}

#[derive(Debug, Clone)]
pub struct StarImportation<'a> {
    /// The level of the import. `None` or `Some(0)` indicate an absolute import.
    pub level: Option<u32>,
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
    NamedExprAssignment,
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
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct Exceptions: u8 {
        const NAME_ERROR = 0b0000_0001;
        const MODULE_NOT_FOUND_ERROR = 0b0000_0010;
        const IMPORT_ERROR = 0b0000_0100;
    }
}
