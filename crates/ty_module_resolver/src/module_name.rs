use std::fmt;
use std::num::NonZeroU32;
use std::ops::Deref;

use compact_str::{CompactString, ToCompactString};

use ruff_db::files::File;
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::Db;
use crate::resolve::file_to_module;
use crate::{ResolverEnvironment, ResolverFile};

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord, get_size2::GetSize)]
pub struct ModuleName(compact_str::CompactString);

impl ModuleName {
    /// Creates a new module name for `name`. Returns `Some` if `name` is a valid, absolute
    /// module name and `None` otherwise.
    ///
    /// The module name is invalid if:
    ///
    /// * The name is empty
    /// * The name is relative
    /// * The name ends with a `.`
    /// * The name contains a sequence of multiple dots
    /// * A component of a name (the part between two dots) isn't a valid python identifier.
    #[inline]
    #[must_use]
    pub fn new(name: &str) -> Option<Self> {
        Self::is_valid_name(name).then(|| Self(CompactString::from(name)))
    }

    /// Creates a new module name for `name` where `name` is a static string.
    /// Returns `Some` if `name` is a valid, absolute module name and `None` otherwise.
    ///
    /// The module name is invalid if:
    ///
    /// * The name is empty
    /// * The name is relative
    /// * The name ends with a `.`
    /// * The name contains a sequence of multiple dots
    /// * A component of a name (the part between two dots) isn't a valid python identifier.
    ///
    /// For example, `foo.bar` is valid, while `.foo`, `foo..bar`, and `2000` are invalid.
    #[inline]
    #[must_use]
    pub fn new_static(name: &'static str) -> Option<Self> {
        Self::is_valid_name(name).then(|| Self(CompactString::const_new(name)))
    }

    #[must_use]
    fn is_valid_name(name: &str) -> bool {
        !name.is_empty() && name.split('.').all(is_identifier)
    }

    /// Returns an iterator over the components of the module name.
    ///
    /// For example, `foo.bar.baz` yields `foo`, `bar`, then `baz`.
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// Returns the first component in this module name.
    #[must_use]
    pub fn first_component(&self) -> &str {
        // OK because `Self::is_valid_name` guarantees that there is at least
        // one component in the module name.
        self.components()
            .next()
            .expect("at least one module component")
    }

    /// Returns the last component in this module name.
    #[must_use]
    pub fn last_component(&self) -> &str {
        // OK because `Self::is_valid_name` guarantees that there is at least
        // one component in the module name.
        self.components()
            .next_back()
            .expect("at least one module component")
    }

    /// The name of this module's immediate parent, if it has a parent.
    ///
    /// For example, the parent of `foo.bar` is `foo`. A top-level module has no parent.
    #[must_use]
    pub fn parent(&self) -> Option<ModuleName> {
        let (parent, _) = self.0.rsplit_once('.')?;
        Some(Self(parent.to_compact_string()))
    }

    /// Returns `true` if the name starts with `other`.
    ///
    /// This is equivalent to checking if `self` is a sub-module of `other`.
    ///
    /// For example, `foo.bar` starts with `foo`, while `foo_bar` does not.
    #[must_use]
    pub fn starts_with(&self, other: &ModuleName) -> bool {
        let mut self_components = self.components();
        let other_components = other.components();

        for other_component in other_components {
            if self_components.next() != Some(other_component) {
                return false;
            }
        }

        true
    }

    /// Given a parent module name of this module name, return the relative
    /// portion of this module name.
    ///
    /// For example, a parent module name of `importlib` with this module name
    /// as `importlib.resources`, this returns `resources`.
    ///
    /// If `parent` isn't a parent name of this module name, then this returns
    /// `None`.
    ///
    /// Parent names must match whole components. For example, `importlib` is not a parent
    /// of `importliblib.resources`.
    #[must_use]
    pub fn relative_to(&self, parent: &ModuleName) -> Option<ModuleName> {
        let relative_name = self.0.strip_prefix(&*parent.0)?.strip_prefix('.')?;
        // At this point, `relative_name` *has* to be a
        // proper suffix of `self`. Otherwise, one of the two
        // `strip_prefix` calls above would return `None`.
        // (Notably, a valid `ModuleName` cannot end with a `.`.)
        assert!(!relative_name.is_empty());
        // This must also be true for this implementation to be
        // correct. That is, the parent must be a prefix of this
        // module name according to the rules of how module name
        // components are split up. This could technically trip if
        // the implementation of `starts_with` diverges from the
        // implementation in this routine. But that seems unlikely.
        debug_assert!(self.starts_with(parent));
        Some(ModuleName(CompactString::from(relative_name)))
    }

    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Construct a [`ModuleName`] from a sequence of parts.
    ///
    /// For example, `["a", "b"]` produces `a.b`. Returns `None` for an empty sequence or
    /// if any component is an invalid identifier.
    #[must_use]
    pub fn from_components<'a>(components: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut components = components.into_iter();
        let first_part = components.next()?;
        if !is_identifier(first_part) {
            return None;
        }
        let mut name = CompactString::from(first_part);
        for part in components {
            if !is_identifier(part) {
                return None;
            }
            name.push('.');
            name.push_str(part);
        }
        Some(Self(name))
    }

    /// Extends `self` with the components of `other`.
    ///
    /// For example, extending `foo.bar` with `baz.eggs` produces `foo.bar.baz.eggs`.
    pub fn extend(&mut self, other: &ModuleName) {
        self.0.push('.');
        self.0.push_str(other);
    }

    /// Returns an iterator of this module name and all of its parent modules.
    ///
    /// For example, `foo.bar.baz` yields `foo.bar.baz`, `foo.bar`, then `foo`.
    pub fn ancestors(&self) -> impl Iterator<Item = Self> {
        std::iter::successors(Some(self.clone()), Self::parent)
    }

    /// Extracts a module name from the AST of a `from <module> import ...`
    /// statement.
    ///
    /// `importing_file` must be the file that contains the import statement.
    ///
    /// This handles relative import statements.
    pub fn from_import_statement<'db>(
        db: &'db dyn Db,
        importing_file: ImportingFile<'db>,
        node: &ast::StmtImportFrom,
    ) -> Result<Self, ModuleNameResolutionError> {
        let ast::StmtImportFrom {
            module,
            level,
            names: _,
            is_lazy: _,
            range: _,
            node_index: _,
        } = node;
        Self::from_identifier_parts(db, importing_file, module.as_deref(), *level)
    }

    /// Computes the absolute module name from the LHS components of `from LHS import RHS`
    pub fn from_identifier_parts<'db>(
        db: &'db dyn Db,
        importing_file: ImportingFile<'db>,
        module: Option<&str>,
        level: u32,
    ) -> Result<Self, ModuleNameResolutionError> {
        if let Some(level) = NonZeroU32::new(level) {
            relative_module_name(db, importing_file.resolver_file(db), module, level)
        } else {
            module
                .and_then(Self::new)
                .ok_or(ModuleNameResolutionError::InvalidSyntax)
        }
    }

    /// Computes the absolute module name for the package this file belongs to.
    ///
    /// i.e. this resolves `.`
    pub fn package_for_file<'db>(
        db: &'db dyn Db,
        importing_file: ImportingFile<'db>,
    ) -> Result<Self, ModuleNameResolutionError> {
        Self::from_identifier_parts(db, importing_file, None, 1)
    }

    /// Returns `true` if the module name given appears to be a test module.
    ///
    /// This routine is meant to codify a Python ecosystem convention. That is,
    /// a module is considered a test module if any of the following are true:
    ///
    /// * Any non-root component is `test` or `tests`
    ///   (e.g., `numpy.tests.test_core`).
    /// * The final component is `conftest` (pytest configuration).
    ///
    /// Note that top-level "testing" modules like `pandas.testing` are
    /// intentionally not filtered, as they provide utilities meant for external
    /// use.
    ///
    /// # Usage
    ///
    /// Callers should be mindful when using this routine to filter items
    /// presented to end users. For example, auto-import uses this to filter
    /// completions offered, but only for completions outside of the end
    /// user's first party code. That is, end users still expect to see
    /// suggestions from their own test modules, but not for test modules in
    /// their dependencies.
    pub fn is_test_module(&self) -> bool {
        if self.last_component() == "conftest" {
            return true;
        }
        self.components()
            .skip(1)
            .any(|c| c == "test" || c == "tests")
    }

    /// Returns `true` if the module name is considered private.
    ///
    /// This routine is meant to codify a Python ecosystem convention. That is,
    /// a module is considered private if itself or any of its parent modules
    /// starts with a `_`.
    ///
    /// # Usage
    ///
    /// Callers should be mindful when using this routine to filter items
    /// presented to end users. For example, auto-import uses this to filter
    /// completions offered, but only for completions outside of the end user's
    /// first party code. That is, end users still expect to see suggestions
    /// from their private modules, but not for private modules in their
    /// dependencies.
    pub fn is_private(&self) -> bool {
        self.components().any(|c| c.starts_with('_'))
    }
}

impl Deref for ModuleName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<str> for ModuleName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<ModuleName> for str {
    fn eq(&self, other: &ModuleName) -> bool {
        self == other.as_str()
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The file from which an import is resolved.
///
/// Most absolute imports only need the resolver environment. Creating a [`ResolverFile`] for each
/// such import would unnecessarily intern the file and environment together, even though that
/// combined identity is never used:
///
/// ```text
/// resolve_module(ImportingFile::File(shared.py, environment), "dependency")
///     -> resolve using environment; no ResolverFile needed
/// ```
///
/// Relative imports, on the other hand, need the importing file's module identity and therefore
/// require a [`ResolverFile`]:
///
/// ```text
/// from .dependency import value
///     -> importing_file.resolver_file(db)
///     -> ResolverFile(shared.py, environment)
/// ```
///
/// [`ImportingFile::File`] defers interning until such a code path actually calls
/// [`ImportingFile::resolver_file`]. Callers that already have an interned resolver file can pass
/// [`ImportingFile::ResolverFile`] to reuse it directly.
#[derive(Clone, Copy)]
pub enum ImportingFile<'db> {
    /// An already-interned resolver key that can be reused without materialization.
    ResolverFile(ResolverFile<'db>),
    /// An importing file and resolver environment whose combined key is materialized lazily.
    File(File, ResolverEnvironment<'db>),
}

impl<'db> ImportingFile<'db> {
    pub fn file(self, db: &dyn Db) -> File {
        match self {
            Self::ResolverFile(file) => file.file(db),
            Self::File(file, _) => file,
        }
    }

    pub(crate) fn resolver_environment(self, db: &'db dyn Db) -> ResolverEnvironment<'db> {
        match self {
            Self::ResolverFile(file) => file.environment(db),
            Self::File(_, resolver_environment) => resolver_environment,
        }
    }

    pub fn python_version(self, db: &'db dyn Db) -> PythonVersion {
        self.resolver_environment(db).python_version(db)
    }

    /// Returns the existing resolver key or materializes one when required.
    pub fn resolver_file(self, db: &'db dyn Db) -> ResolverFile<'db> {
        match self {
            Self::ResolverFile(file) => file,
            Self::File(file, resolver_environment) => {
                ResolverFile::new(db, file, resolver_environment)
            }
        }
    }
}

/// Given a `from .foo import bar` relative import, resolve the relative module
/// we're importing `bar` from into an absolute [`ModuleName`]
/// using the name of the module we're currently analyzing.
///
/// - `level` is the number of dots at the beginning of the relative module name:
///   - `from .foo.bar import baz` => `level == 1`
///   - `from ...foo.bar import baz` => `level == 3`
/// - `tail` is the relative module name stripped of all leading dots:
///   - `from .foo import bar` => `tail == "foo"`
///   - `from ..foo.bar import baz` => `tail == "foo.bar"`
fn relative_module_name<'db>(
    db: &'db dyn Db,
    importing_file: ResolverFile<'db>,
    tail: Option<&str>,
    level: NonZeroU32,
) -> Result<ModuleName, ModuleNameResolutionError> {
    let module = file_to_module(db, importing_file)
        .ok_or(ModuleNameResolutionError::UnknownCurrentModule)?;
    let mut level = level.get();

    if module.kind(db).is_package() {
        level = level.saturating_sub(1);
    }

    let mut module_name = module
        .name(db)
        .ancestors()
        .nth(level as usize)
        .ok_or(ModuleNameResolutionError::TooManyDots)?;

    if let Some(tail) = tail {
        let tail = ModuleName::new(tail).ok_or(ModuleNameResolutionError::InvalidSyntax)?;
        module_name.extend(&tail);
    }

    Ok(module_name)
}

/// Various ways in which resolving a [`ModuleName`]
/// from an [`ast::StmtImport`] or [`ast::StmtImportFrom`] node might fail
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ModuleNameResolutionError {
    /// The import statement has invalid syntax
    InvalidSyntax,

    /// We couldn't resolve the file we're currently analyzing back to a module
    /// (Only necessary for relative import statements)
    UnknownCurrentModule,

    /// The relative import statement seems to take us outside of the module search path
    /// (e.g. our current module is `foo.bar`, and the relative import statement in `foo.bar`
    /// is `from ....baz import spam`)
    TooManyDots,
}
