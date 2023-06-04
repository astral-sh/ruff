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
            BindingKind::Importation(Importation { qualified_name }) => {
                if let BindingKind::SubmoduleImportation(SubmoduleImportation {
                    qualified_name: existing,
                }) = &existing.kind
                {
                    return qualified_name == existing;
                }
            }
            BindingKind::FromImportation(FromImportation { qualified_name }) => {
                if let BindingKind::SubmoduleImportation(SubmoduleImportation {
                    qualified_name: existing,
                }) = &existing.kind
                {
                    return qualified_name == existing;
                }
            }
            BindingKind::SubmoduleImportation(SubmoduleImportation { qualified_name }) => {
                match &existing.kind {
                    BindingKind::Importation(Importation {
                        qualified_name: existing,
                    })
                    | BindingKind::SubmoduleImportation(SubmoduleImportation {
                        qualified_name: existing,
                    }) => {
                        return qualified_name == existing;
                    }
                    BindingKind::FromImportation(FromImportation {
                        qualified_name: existing,
                    }) => {
                        return qualified_name == existing;
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
                | BindingKind::Importation(..)
                | BindingKind::FromImportation(..)
                | BindingKind::SubmoduleImportation(..)
        )
    }

    /// Returns the fully-qualified symbol name, if this symbol was imported from another module.
    pub fn qualified_name(&self) -> Option<&str> {
        match &self.kind {
            BindingKind::Importation(Importation { qualified_name }) => Some(qualified_name),
            BindingKind::FromImportation(FromImportation { qualified_name }) => {
                Some(qualified_name)
            }
            BindingKind::SubmoduleImportation(SubmoduleImportation { qualified_name }) => {
                Some(qualified_name)
            }
            _ => None,
        }
    }

    /// Returns the fully-qualified name of the module from which this symbol was imported, if this
    /// symbol was imported from another module.
    pub fn module_name(&self) -> Option<&str> {
        match &self.kind {
            BindingKind::Importation(Importation { qualified_name })
            | BindingKind::SubmoduleImportation(SubmoduleImportation { qualified_name }) => {
                Some(qualified_name.split('.').next().unwrap_or(qualified_name))
            }
            BindingKind::FromImportation(FromImportation { qualified_name }) => Some(
                qualified_name
                    .rsplit_once('.')
                    .map_or(qualified_name, |(module, _)| module),
            ),
            _ => None,
        }
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

/// A binding for an `import`, keyed on the name to which the import is bound.
/// Ex) `import foo` would be keyed on "foo".
/// Ex) `import foo as bar` would be keyed on "bar".
#[derive(Clone, Debug)]
pub struct Importation<'a> {
    /// The full name of the module being imported.
    /// Ex) Given `import foo`, `qualified_name` would be "foo".
    /// Ex) Given `import foo as bar`, `qualified_name` would be "foo".
    pub qualified_name: &'a str,
}

/// A binding for a member imported from a module, keyed on the name to which the member is bound.
/// Ex) `from foo import bar` would be keyed on "bar".
/// Ex) `from foo import bar as baz` would be keyed on "baz".
#[derive(Clone, Debug)]
pub struct FromImportation {
    /// The full name of the member being imported.
    /// Ex) Given `from foo import bar`, `qualified_name` would be "foo.bar".
    /// Ex) Given `from foo import bar as baz`, `qualified_name` would be "foo.bar".
    pub qualified_name: String,
}

/// A binding for a submodule imported from a module, keyed on the name of the parent module.
/// Ex) `import foo.bar` would be keyed on "foo".
#[derive(Clone, Debug)]
pub struct SubmoduleImportation<'a> {
    /// The full name of the submodule being imported.
    /// Ex) Given `import foo.bar`, `qualified_name` would be "foo.bar".
    pub qualified_name: &'a str,
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
    FromImportation(FromImportation),
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
