use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

use bitflags::bitflags;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_python_ast::call_path::format_call_path;
use ruff_python_ast::Stmt;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::context::ExecutionContext;
use crate::model::SemanticModel;
use crate::nodes::NodeId;
use crate::reference::ResolvedReferenceId;
use crate::ScopeId;

#[derive(Debug, Clone)]
pub struct Binding<'a> {
    pub kind: BindingKind<'a>,
    pub range: TextRange,
    /// The [`ScopeId`] of the scope in which the [`Binding`] was defined.
    pub scope: ScopeId,
    /// The context in which the [`Binding`] was created.
    pub context: ExecutionContext,
    /// The statement in which the [`Binding`] was defined.
    pub source: Option<NodeId>,
    /// The references to the [`Binding`].
    pub references: Vec<ResolvedReferenceId>,
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
    pub fn references(&self) -> impl Iterator<Item = ResolvedReferenceId> + '_ {
        self.references.iter().copied()
    }

    /// Return `true` if this [`Binding`] represents an explicit re-export
    /// (e.g., `FastAPI` in `from fastapi import FastAPI as FastAPI`).
    pub const fn is_explicit_export(&self) -> bool {
        self.flags.intersects(BindingFlags::EXPLICIT_EXPORT)
    }

    /// Return `true` if this [`Binding`] represents an external symbol
    /// (e.g., `FastAPI` in `from fastapi import FastAPI`).
    pub const fn is_external(&self) -> bool {
        self.flags.intersects(BindingFlags::EXTERNAL)
    }

    /// Return `true` if this [`Binding`] represents an aliased symbol
    /// (e.g., `app` in `from fastapi import FastAPI as app`).
    pub const fn is_alias(&self) -> bool {
        self.flags.intersects(BindingFlags::ALIAS)
    }

    /// Return `true` if this [`Binding`] represents a `nonlocal`. A [`Binding`] is a `nonlocal`
    /// if it's declared by a `nonlocal` statement, or shadows a [`Binding`] declared by a
    /// `nonlocal` statement.
    pub const fn is_nonlocal(&self) -> bool {
        self.flags.intersects(BindingFlags::NONLOCAL)
    }

    /// Return `true` if this [`Binding`] represents a `global`. A [`Binding`] is a `global` if it's
    /// declared by a `global` statement, or shadows a [`Binding`] declared by a `global` statement.
    pub const fn is_global(&self) -> bool {
        self.flags.intersects(BindingFlags::GLOBAL)
    }

    /// Return `true` if this [`Binding`] represents an assignment to `__all__` with an invalid
    /// value (e.g., `__all__ = "Foo"`).
    pub const fn is_invalid_all_format(&self) -> bool {
        self.flags.intersects(BindingFlags::INVALID_ALL_FORMAT)
    }

    /// Return `true` if this [`Binding`] represents an assignment to `__all__` that includes an
    /// invalid member (e.g., `__all__ = ["Foo", 1]`).
    pub const fn is_invalid_all_object(&self) -> bool {
        self.flags.intersects(BindingFlags::INVALID_ALL_OBJECT)
    }

    /// Return `true` if this [`Binding`] represents an unbound variable
    /// (e.g., `x` in `x = 1; del x`).
    pub const fn is_unbound(&self) -> bool {
        matches!(
            self.kind,
            BindingKind::Annotation | BindingKind::Deletion | BindingKind::UnboundException(_)
        )
    }

    /// Return `true` if this [`Binding`] represents an private declaration
    /// (e.g., `_x` in `_x = "private variable"`)
    pub const fn is_private_declaration(&self) -> bool {
        self.flags.contains(BindingFlags::PRIVATE_DECLARATION)
    }

    /// Return `true` if this binding "redefines" the given binding, as per Pyflake's definition of
    /// redefinition.
    pub fn redefines(&self, existing: &'a Binding) -> bool {
        match &self.kind {
            // Submodule imports are only considered redefinitions if they import the same
            // submodule. For example, this is a redefinition:
            // ```python
            // import foo.bar
            // import foo.bar
            // ```
            //
            // This, however, is not:
            // ```python
            // import foo.bar
            // import foo.baz
            // ```
            BindingKind::Import(Import {
                call_path: redefinition,
            }) => {
                if let BindingKind::SubmoduleImport(SubmoduleImport {
                    call_path: definition,
                }) = &existing.kind
                {
                    return redefinition == definition;
                }
            }
            BindingKind::FromImport(FromImport {
                call_path: redefinition,
            }) => {
                if let BindingKind::SubmoduleImport(SubmoduleImport {
                    call_path: definition,
                }) = &existing.kind
                {
                    return redefinition == definition;
                }
            }
            BindingKind::SubmoduleImport(SubmoduleImport {
                call_path: redefinition,
            }) => match &existing.kind {
                BindingKind::Import(Import {
                    call_path: definition,
                })
                | BindingKind::SubmoduleImport(SubmoduleImport {
                    call_path: definition,
                }) => {
                    return redefinition == definition;
                }
                BindingKind::FromImport(FromImport {
                    call_path: definition,
                }) => {
                    return redefinition == definition;
                }
                _ => {}
            },
            // Deletions, annotations, `__future__` imports, and builtins are never considered
            // redefinitions.
            BindingKind::Deletion
            | BindingKind::Annotation
            | BindingKind::FutureImport
            | BindingKind::Builtin => {
                return false;
            }
            _ => {}
        }
        // Otherwise, the shadowed binding must be a class definition, function definition, or
        // import to be considered a redefinition.
        matches!(
            existing.kind,
            BindingKind::ClassDefinition(_)
                | BindingKind::FunctionDefinition(_)
                | BindingKind::Import(_)
                | BindingKind::FromImport(_)
        )
    }

    /// Returns the name of the binding (e.g., `x` in `x = 1`).
    pub fn name<'b>(&self, locator: &'b Locator) -> &'b str {
        locator.slice(self.range)
    }

    /// Returns the statement in which the binding was defined.
    pub fn statement<'b>(&self, semantic: &'b SemanticModel) -> Option<&'b Stmt> {
        self.source
            .map(|statement_id| semantic.statement(statement_id))
    }

    /// Returns the range of the binding's parent.
    pub fn parent_range(&self, semantic: &SemanticModel) -> Option<TextRange> {
        self.statement(semantic).and_then(|parent| {
            if parent.is_import_from_stmt() {
                Some(parent.range())
            } else {
                None
            }
        })
    }

    pub fn as_any_import(&'a self) -> Option<AnyImport<'a>> {
        match &self.kind {
            BindingKind::Import(import) => Some(AnyImport::Import(import)),
            BindingKind::SubmoduleImport(import) => Some(AnyImport::SubmoduleImport(import)),
            BindingKind::FromImport(import) => Some(AnyImport::FromImport(import)),
            _ => None,
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
        /// from fastapi import FastAPI as FastAPI
        /// ```
        const EXPLICIT_EXPORT = 1 << 0;

        /// The binding represents an external symbol, like an import or a builtin.
        ///
        /// For example, the binding could be `FastAPI` in:
        /// ```python
        /// from fastapi import FastAPI
        /// ```
        const EXTERNAL = 1 << 1;

        /// The binding is an aliased symbol.
        ///
        /// For example, the binding could be `app` in:
        /// ```python
        /// from fastapi import FastAPI as app
        /// ```
        const ALIAS = 1 << 2;

        /// The binding is `nonlocal` to the declaring scope. This could be a binding created by
        /// a `nonlocal` statement, or a binding that shadows such a binding.
        ///
        /// For example, both of the bindings in the following function are `nonlocal`:
        /// ```python
        /// def f():
        ///     nonlocal x
        ///     x = 1
        /// ```
        const NONLOCAL = 1 << 3;

        /// The binding is `global`. This could be a binding created by a `global` statement, or a
        /// binding that shadows such a binding.
        ///
        /// For example, both of the bindings in the following function are `global`:
        /// ```python
        /// def f():
        ///     global x
        ///     x = 1
        /// ```
        const GLOBAL = 1 << 4;

        /// The binding represents an export via `__all__`, but the assigned value uses an invalid
        /// expression (i.e., a non-container type).
        ///
        /// For example:
        /// ```python
        /// __all__ = 1
        /// ```
        const INVALID_ALL_FORMAT = 1 << 5;

        /// The binding represents an export via `__all__`, but the assigned value contains an
        /// invalid member (i.e., a non-string).
        ///
        /// For example:
        /// ```python
        /// __all__ = [1]
        /// ```
        const INVALID_ALL_OBJECT = 1 << 6;

        /// The binding represents a private declaration.
        ///
        /// For example, the binding could be `_T` in:
        /// ```python
        /// _T = "This is a private variable"
        /// ```
        const PRIVATE_DECLARATION = 1 << 7;
    }
}

impl Ranged for Binding<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// ID uniquely identifying a [Binding] in a program.
///
/// Using a `u32` to identify [Binding]s should is sufficient because Ruff only supports documents with a
/// size smaller than or equal to `u32::max`. A document with the size of `u32::max` must have fewer than `u32::max`
/// bindings because bindings must be separated by whitespace (and have an assignment).
#[newtype_index]
pub struct BindingId;

/// The bindings in a program.
///
/// Bindings are indexed by [`BindingId`]
#[derive(Debug, Clone, Default)]
pub struct Bindings<'a>(IndexVec<BindingId, Binding<'a>>);

impl<'a> Bindings<'a> {
    /// Pushes a new [`Binding`] and returns its [`BindingId`].
    pub fn push(&mut self, binding: Binding<'a>) -> BindingId {
        self.0.push(binding)
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
pub struct Export<'a> {
    /// The names of the bindings exported via `__all__`.
    pub names: Box<[&'a str]>,
}

/// A binding for an `import`, keyed on the name to which the import is bound.
/// Ex) `import foo` would be keyed on "foo".
/// Ex) `import foo as bar` would be keyed on "bar".
#[derive(Debug, Clone)]
pub struct Import<'a> {
    /// The full name of the module being imported.
    /// Ex) Given `import foo`, `qualified_name` would be "foo".
    /// Ex) Given `import foo as bar`, `qualified_name` would be "foo".
    pub call_path: Box<[&'a str]>,
}

/// A binding for a member imported from a module, keyed on the name to which the member is bound.
/// Ex) `from foo import bar` would be keyed on "bar".
/// Ex) `from foo import bar as baz` would be keyed on "baz".
#[derive(Debug, Clone)]
pub struct FromImport<'a> {
    /// The full name of the member being imported.
    /// Ex) Given `from foo import bar`, `qualified_name` would be "foo.bar".
    /// Ex) Given `from foo import bar as baz`, `qualified_name` would be "foo.bar".
    pub call_path: Box<[&'a str]>,
}

/// A binding for a submodule imported from a module, keyed on the name of the parent module.
/// Ex) `import foo.bar` would be keyed on "foo".
#[derive(Debug, Clone)]
pub struct SubmoduleImport<'a> {
    /// The full name of the submodule being imported.
    /// Ex) Given `import foo.bar`, `qualified_name` would be "foo.bar".
    pub call_path: Box<[&'a str]>,
}

#[derive(Debug, Clone, is_macro::Is)]
pub enum BindingKind<'a> {
    /// A binding for an annotated assignment without a value, like `x` in:
    /// ```python
    /// x: int
    /// ```
    Annotation,

    /// A binding for a function argument, like `x` in:
    /// ```python
    /// def foo(x: int):
    ///     ...
    /// ```
    Argument,

    /// A binding for a named expression assignment, like `x` in:
    /// ```python
    /// if (x := 1):
    ///     ...
    /// ```
    NamedExprAssignment,

    /// A binding for a unpacking-based assignment, like `x` in:
    /// ```python
    /// x, y = (1, 2)
    /// ```
    UnpackedAssignment,

    /// A binding for a "standard" assignment, like `x` in:
    /// ```python
    /// x = 1
    /// ```
    Assignment,

    /// A binding for a generic type parameter, like `X` in:
    /// ```python
    /// def foo[X](x: X):
    ///    ...
    ///
    /// class Foo[X](x: X):
    ///   ...
    ///
    /// type Foo[X] = ...
    /// ```
    TypeParam,

    /// A binding for a for-loop variable, like `x` in:
    /// ```python
    /// for x in range(10):
    ///     ...
    /// ```
    LoopVar,

    /// A binding for a global variable, like `x` in:
    /// ```python
    /// def foo():
    ///     global x
    /// ```
    Global,

    /// A binding for a nonlocal variable, like `x` in:
    /// ```python
    /// def foo():
    ///     nonlocal x
    /// ```
    Nonlocal(ScopeId),

    /// A binding for a builtin, like `print` or `bool`.
    Builtin,

    /// A binding for a class, like `Foo` in:
    /// ```python
    /// class Foo:
    ///     ...
    /// ```
    ClassDefinition(ScopeId),

    /// A binding for a function, like `foo` in:
    /// ```python
    /// def foo():
    ///     ...
    /// ```
    FunctionDefinition(ScopeId),

    /// A binding for an `__all__` export, like `__all__` in:
    /// ```python
    /// __all__ = ["foo", "bar"]
    /// ```
    Export(Export<'a>),

    /// A binding for a `__future__` import, like:
    /// ```python
    /// from __future__ import annotations
    /// ```
    FutureImport,

    /// A binding for a straight `import`, like `foo` in:
    /// ```python
    /// import foo
    /// ```
    Import(Import<'a>),

    /// A binding for a member imported from a module, like `bar` in:
    /// ```python
    /// from foo import bar
    /// ```
    FromImport(FromImport<'a>),

    /// A binding for a submodule imported from a module, like `bar` in:
    /// ```python
    /// import foo.bar
    /// ```
    SubmoduleImport(SubmoduleImport<'a>),

    /// A binding for a deletion, like `x` in:
    /// ```python
    /// del x
    /// ```
    Deletion,

    /// A binding to bind an exception to a local variable, like `x` in:
    /// ```python
    /// try:
    ///    ...
    /// except Exception as x:
    ///   ...
    /// ```
    BoundException,

    /// A binding to unbind a bound local exception, like `x` in:
    /// ```python
    /// try:
    ///    ...
    /// except Exception as x:
    ///   ...
    /// ```
    ///
    /// After the `except` block, `x` is unbound, despite the lack
    /// of an explicit `del` statement.
    ///
    /// Stores the ID of the binding that was shadowed in the enclosing
    /// scope, if any.
    UnboundException(Option<BindingId>),
}

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
    pub struct Exceptions: u8 {
        const NAME_ERROR = 0b0000_0001;
        const MODULE_NOT_FOUND_ERROR = 0b0000_0010;
        const IMPORT_ERROR = 0b0000_0100;
    }
}

/// A trait for imported symbols.
pub trait Imported<'a> {
    /// Returns the call path to the imported symbol.
    fn call_path(&self) -> &[&str];

    /// Returns the module name of the imported symbol.
    fn module_name(&self) -> &[&str];

    /// Returns the member name of the imported symbol. For a straight import, this is equivalent
    /// to the qualified name; for a `from` import, this is the name of the imported symbol.
    fn member_name(&self) -> Cow<'a, str>;

    /// Returns the fully-qualified name of the imported symbol.
    fn qualified_name(&self) -> String {
        format_call_path(self.call_path())
    }
}

impl<'a> Imported<'a> for Import<'a> {
    /// For example, given `import foo`, returns `["foo"]`.
    fn call_path(&self) -> &[&str] {
        self.call_path.as_ref()
    }

    /// For example, given `import foo`, returns `["foo"]`.
    fn module_name(&self) -> &[&str] {
        &self.call_path[..1]
    }

    /// For example, given `import foo`, returns `"foo"`.
    fn member_name(&self) -> Cow<'a, str> {
        Cow::Owned(self.qualified_name())
    }
}

impl<'a> Imported<'a> for SubmoduleImport<'a> {
    /// For example, given `import foo.bar`, returns `["foo", "bar"]`.
    fn call_path(&self) -> &[&str] {
        self.call_path.as_ref()
    }

    /// For example, given `import foo.bar`, returns `["foo"]`.
    fn module_name(&self) -> &[&str] {
        &self.call_path[..1]
    }

    /// For example, given `import foo.bar`, returns `"foo.bar"`.
    fn member_name(&self) -> Cow<'a, str> {
        Cow::Owned(self.qualified_name())
    }
}

impl<'a> Imported<'a> for FromImport<'a> {
    /// For example, given `from foo import bar`, returns `["foo", "bar"]`.
    fn call_path(&self) -> &[&str] {
        self.call_path.as_ref()
    }

    /// For example, given `from foo import bar`, returns `["foo"]`.
    fn module_name(&self) -> &[&str] {
        &self.call_path[..self.call_path.len() - 1]
    }

    /// For example, given `from foo import bar`, returns `"bar"`.
    fn member_name(&self) -> Cow<'a, str> {
        Cow::Borrowed(self.call_path[self.call_path.len() - 1])
    }
}

/// A wrapper around an import [`BindingKind`] that can be any of the three types of imports.
#[derive(Debug, Clone, is_macro::Is)]
pub enum AnyImport<'a> {
    Import(&'a Import<'a>),
    SubmoduleImport(&'a SubmoduleImport<'a>),
    FromImport(&'a FromImport<'a>),
}

impl<'a> Imported<'a> for AnyImport<'a> {
    fn call_path(&self) -> &[&str] {
        match self {
            Self::Import(import) => import.call_path(),
            Self::SubmoduleImport(import) => import.call_path(),
            Self::FromImport(import) => import.call_path(),
        }
    }

    fn module_name(&self) -> &[&str] {
        match self {
            Self::Import(import) => import.module_name(),
            Self::SubmoduleImport(import) => import.module_name(),
            Self::FromImport(import) => import.module_name(),
        }
    }

    fn member_name(&self) -> Cow<'a, str> {
        match self {
            Self::Import(import) => import.member_name(),
            Self::SubmoduleImport(import) => import.member_name(),
            Self::FromImport(import) => import.member_name(),
        }
    }
}
