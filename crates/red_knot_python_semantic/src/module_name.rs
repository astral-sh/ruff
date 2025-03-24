use std::fmt;
use std::num::NonZeroU32;
use std::ops::Deref;

use compact_str::{CompactString, ToCompactString};

use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::{db::Db, module_resolver::file_to_module};

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
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
    /// ## Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar").as_deref(), Some("foo.bar"));
    /// assert_eq!(ModuleName::new_static(""), None);
    /// assert_eq!(ModuleName::new_static("..foo"), None);
    /// assert_eq!(ModuleName::new_static(".foo"), None);
    /// assert_eq!(ModuleName::new_static("foo."), None);
    /// assert_eq!(ModuleName::new_static("foo..bar"), None);
    /// assert_eq!(ModuleName::new_static("2000"), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn new_static(name: &'static str) -> Option<Self> {
        Self::is_valid_name(name).then(|| Self(CompactString::const_new(name)))
    }

    #[must_use]
    fn is_valid_name(name: &str) -> bool {
        !name.is_empty() && name.split('.').all(is_identifier)
    }

    /// An iterator over the components of the module name:
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar.baz").unwrap().components().collect::<Vec<_>>(), vec!["foo", "bar", "baz"]);
    /// ```
    #[must_use]
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// The name of this module's immediate parent, if it has a parent.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar").unwrap().parent(), Some(ModuleName::new_static("foo").unwrap()));
    /// assert_eq!(ModuleName::new_static("foo.bar.baz").unwrap().parent(), Some(ModuleName::new_static("foo.bar").unwrap()));
    /// assert_eq!(ModuleName::new_static("root").unwrap().parent(), None);
    /// ```
    #[must_use]
    pub fn parent(&self) -> Option<ModuleName> {
        let (parent, _) = self.0.rsplit_once('.')?;
        Some(Self(parent.to_compact_string()))
    }

    /// Returns `true` if the name starts with `other`.
    ///
    /// This is equivalent to checking if `self` is a sub-module of `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert!(ModuleName::new_static("foo.bar").unwrap().starts_with(&ModuleName::new_static("foo").unwrap()));
    ///
    /// assert!(!ModuleName::new_static("foo.bar").unwrap().starts_with(&ModuleName::new_static("bar").unwrap()));
    /// assert!(!ModuleName::new_static("foo_bar").unwrap().starts_with(&ModuleName::new_static("foo").unwrap()));
    /// ```
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

    #[must_use]
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Construct a [`ModuleName`] from a sequence of parts.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert_eq!(&*ModuleName::from_components(["a"]).unwrap(), "a");
    /// assert_eq!(&*ModuleName::from_components(["a", "b"]).unwrap(), "a.b");
    /// assert_eq!(&*ModuleName::from_components(["a", "b", "c"]).unwrap(), "a.b.c");
    ///
    /// assert_eq!(ModuleName::from_components(["a-b"]), None);
    /// assert_eq!(ModuleName::from_components(["a", "a-b"]), None);
    /// assert_eq!(ModuleName::from_components(["a", "b", "a-b-c"]), None);
    /// ```
    #[must_use]
    pub fn from_components<'a>(components: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut components = components.into_iter();
        let first_part = components.next()?;
        if !is_identifier(first_part) {
            return None;
        }
        let name = if let Some(second_part) = components.next() {
            if !is_identifier(second_part) {
                return None;
            }
            let mut name = format!("{first_part}.{second_part}");
            for part in components {
                if !is_identifier(part) {
                    return None;
                }
                name.push('.');
                name.push_str(part);
            }
            CompactString::from(&name)
        } else {
            CompactString::from(first_part)
        };
        Some(Self(name))
    }

    /// Extend `self` with the components of `other`
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// let mut module_name = ModuleName::new_static("foo").unwrap();
    /// module_name.extend(&ModuleName::new_static("bar").unwrap());
    /// assert_eq!(&module_name, "foo.bar");
    /// module_name.extend(&ModuleName::new_static("baz.eggs.ham").unwrap());
    /// assert_eq!(&module_name, "foo.bar.baz.eggs.ham");
    /// ```
    pub fn extend(&mut self, other: &ModuleName) {
        self.0.push('.');
        self.0.push_str(other);
    }

    /// Returns an iterator of this module name and all of its parent modules.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_python_semantic::ModuleName;
    ///
    /// assert_eq!(
    ///     ModuleName::new_static("foo.bar.baz").unwrap().ancestors().collect::<Vec<_>>(),
    ///     vec![
    ///         ModuleName::new_static("foo.bar.baz").unwrap(),
    ///         ModuleName::new_static("foo.bar").unwrap(),
    ///         ModuleName::new_static("foo").unwrap(),
    ///     ],
    /// );
    /// ```
    pub fn ancestors(&self) -> impl Iterator<Item = Self> {
        std::iter::successors(Some(self.clone()), Self::parent)
    }

    pub(crate) fn from_import_statement<'db>(
        db: &'db dyn Db,
        importing_file: File,
        node: &'db ast::StmtImportFrom,
    ) -> Result<Self, ModuleNameResolutionError> {
        let ast::StmtImportFrom {
            module,
            level,
            names: _,
            range: _,
        } = node;

        let module = module.as_deref();

        if let Some(level) = NonZeroU32::new(*level) {
            relative_module_name(db, importing_file, module, level)
        } else {
            module
                .and_then(Self::new)
                .ok_or(ModuleNameResolutionError::InvalidSyntax)
        }
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
fn relative_module_name(
    db: &dyn Db,
    importing_file: File,
    tail: Option<&str>,
    level: NonZeroU32,
) -> Result<ModuleName, ModuleNameResolutionError> {
    let module = file_to_module(db, importing_file)
        .ok_or(ModuleNameResolutionError::UnknownCurrentModule)?;
    let mut level = level.get();

    if module.kind().is_package() {
        level = level.saturating_sub(1);
    }

    let mut module_name = module
        .name()
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
pub(crate) enum ModuleNameResolutionError {
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
