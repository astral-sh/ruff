use std::fmt::Formatter;
use std::str::FromStr;

use ruff_db::files::{File, system_path_to_file, vendored_path_to_file};
use ruff_db::system::SystemPath;
use ruff_db::vendored::VendoredPath;
use salsa::Database;
use salsa::plumbing::AsId;

use crate::Db;
use crate::module_name::ModuleName;
use crate::path::{SearchPath, SystemOrVendoredPathRef};

/// Representation of a Python module.
#[derive(Clone, Copy, Eq, Hash, PartialEq, salsa::Supertype, salsa::Update)]
pub enum Module<'db> {
    File(FileModule<'db>),
    Namespace(NamespacePackage<'db>),
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for Module<'_> {}

#[salsa::tracked]
impl<'db> Module<'db> {
    pub(crate) fn file_module(
        db: &'db dyn Db,
        name: ModuleName,
        kind: ModuleKind,
        search_path: SearchPath,
        file: File,
    ) -> Self {
        let known = KnownModule::try_from_search_path_and_name(&search_path, &name);

        Self::File(FileModule::new(db, name, kind, search_path, file, known))
    }

    pub(crate) fn namespace_package(db: &'db dyn Db, name: ModuleName) -> Self {
        Self::Namespace(NamespacePackage::new(db, name))
    }

    /// The absolute name of the module (e.g. `foo.bar`)
    pub fn name(self, db: &'db dyn Database) -> &'db ModuleName {
        match self {
            Module::File(module) => module.name(db),
            Module::Namespace(ref package) => package.name(db),
        }
    }

    /// The file to the source code that defines this module
    ///
    /// This is `None` for namespace packages.
    pub fn file(self, db: &'db dyn Database) -> Option<File> {
        match self {
            Module::File(module) => Some(module.file(db)),
            Module::Namespace(_) => None,
        }
    }

    /// Is this a module that we special-case somehow? If so, which one?
    pub fn known(self, db: &'db dyn Database) -> Option<KnownModule> {
        match self {
            Module::File(module) => module.known(db),
            Module::Namespace(_) => None,
        }
    }

    /// Does this module represent the given known module?
    pub fn is_known(self, db: &'db dyn Database, known_module: KnownModule) -> bool {
        self.known(db) == Some(known_module)
    }

    /// The search path from which the module was resolved.
    ///
    /// It is guaranteed that if `None` is returned, then this is a namespace
    /// package. Otherwise, this is a regular package or file module.
    pub fn search_path(self, db: &'db dyn Database) -> Option<&'db SearchPath> {
        match self {
            Module::File(module) => Some(module.search_path(db)),
            Module::Namespace(_) => None,
        }
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(self, db: &'db dyn Database) -> ModuleKind {
        match self {
            Module::File(module) => module.kind(db),
            Module::Namespace(_) => ModuleKind::Package,
        }
    }

    /// Return a list of all submodules of this module.
    ///
    /// Returns an empty list if the module is not a package, if it is an empty package,
    /// or if it is a namespace package (one without an `__init__.py` or `__init__.pyi` file).
    ///
    /// The names returned correspond to the "base" name of the module.
    /// That is, `{self.name}.{basename}` should give the full module name.
    pub fn all_submodules(self, db: &'db dyn Db) -> &'db [Module<'db>] {
        all_submodule_names_for_package(db, self)
            .as_deref()
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for Module<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        salsa::with_attached_database(|db| {
            f.debug_struct("Module")
                .field("name", &self.name(db))
                .field("kind", &self.kind(db))
                .field("file", &self.file(db))
                .field("search_path", &self.search_path(db))
                .field("known", &self.known(db))
                .finish()
        })
        .unwrap_or_else(|| f.debug_tuple("Module").field(&self.as_id()).finish())
    }
}

#[allow(clippy::ref_option)]
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn all_submodule_names_for_package<'db>(
    db: &'db dyn Db,
    module: Module<'db>,
) -> Option<Vec<Module<'db>>> {
    fn is_submodule(
        is_dir: bool,
        is_file: bool,
        basename: Option<&str>,
        extension: Option<&str>,
    ) -> bool {
        is_dir
            || (is_file
                && matches!(extension, Some("py" | "pyi"))
                && !matches!(basename, Some("__init__.py" | "__init__.pyi")))
    }

    fn find_package_init_system(db: &dyn Db, dir: &SystemPath) -> Option<File> {
        system_path_to_file(db, dir.join("__init__.pyi"))
            .or_else(|_| system_path_to_file(db, dir.join("__init__.py")))
            .ok()
    }

    fn find_package_init_vendored(db: &dyn Db, dir: &VendoredPath) -> Option<File> {
        vendored_path_to_file(db, dir.join("__init__.pyi"))
            .or_else(|_| vendored_path_to_file(db, dir.join("__init__.py")))
            .ok()
    }

    // It would be complex and expensive to compute all submodules for
    // namespace packages, since a namespace package doesn't correspond
    // to a single file; it can span multiple directories across multiple
    // search paths. For now, we only compute submodules for traditional
    // packages that exist in a single directory on a single search path.
    let Module::File(module) = module else {
        return None;
    };
    if !matches!(module.kind(db), ModuleKind::Package) {
        return None;
    }

    let path = SystemOrVendoredPathRef::try_from_file(db, module.file(db))?;
    debug_assert!(
        matches!(path.file_name(), Some("__init__.py" | "__init__.pyi")),
        "expected package file `{:?}` to be `__init__.py` or `__init__.pyi`",
        path.file_name(),
    );

    Some(match path.parent()? {
        SystemOrVendoredPathRef::System(parent_directory) => {
            // Read the revision on the corresponding file root to
            // register an explicit dependency on this directory
            // tree. When the revision gets bumped, the cache
            // that Salsa creates does for this routine will be
            // invalidated.
            let root = db.files().expect_root(db, parent_directory);
            let _ = root.revision(db);

            db.system()
                .read_directory(parent_directory)
                .inspect_err(|err| {
                    tracing::debug!(
                        "Failed to read {parent_directory:?} when looking for \
                         its possible submodules: {err}"
                    );
                })
                .ok()?
                .flatten()
                .filter(|entry| {
                    let ty = entry.file_type();
                    let path = entry.path();
                    is_submodule(
                        ty.is_directory(),
                        ty.is_file(),
                        path.file_name(),
                        path.extension(),
                    )
                })
                .filter_map(|entry| {
                    let stem = entry.path().file_stem()?;
                    let mut name = module.name(db).clone();
                    name.extend(&ModuleName::new(stem)?);

                    let (kind, file) = if entry.file_type().is_directory() {
                        (
                            ModuleKind::Package,
                            find_package_init_system(db, entry.path())?,
                        )
                    } else {
                        let file = system_path_to_file(db, entry.path()).ok()?;
                        (ModuleKind::Module, file)
                    };
                    Some(Module::file_module(
                        db,
                        name,
                        kind,
                        module.search_path(db).clone(),
                        file,
                    ))
                })
                .collect()
        }
        SystemOrVendoredPathRef::Vendored(parent_directory) => db
            .vendored()
            .read_directory(parent_directory)
            .into_iter()
            .filter(|entry| {
                let ty = entry.file_type();
                let path = entry.path();
                is_submodule(
                    ty.is_directory(),
                    ty.is_file(),
                    path.file_name(),
                    path.extension(),
                )
            })
            .filter_map(|entry| {
                let stem = entry.path().file_stem()?;
                let mut name = module.name(db).clone();
                name.extend(&ModuleName::new(stem)?);

                let (kind, file) = if entry.file_type().is_directory() {
                    (
                        ModuleKind::Package,
                        find_package_init_vendored(db, entry.path())?,
                    )
                } else {
                    let file = vendored_path_to_file(db, entry.path()).ok()?;
                    (ModuleKind::Module, file)
                };
                Some(Module::file_module(
                    db,
                    name,
                    kind,
                    module.search_path(db).clone(),
                    file,
                ))
            })
            .collect(),
    })
}

/// A module that resolves to a file (`lib.py` or `package/__init__.py`)
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct FileModule<'db> {
    #[returns(ref)]
    pub(super) name: ModuleName,
    pub(super) kind: ModuleKind,
    #[returns(ref)]
    pub(super) search_path: SearchPath,
    pub(super) file: File,
    pub(super) known: Option<KnownModule>,
}

/// A namespace package.
///
/// Namespace packages are special because there are
/// multiple possible paths and they have no corresponding code file.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct NamespacePackage<'db> {
    #[returns(ref)]
    pub(super) name: ModuleName,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub enum ModuleKind {
    /// A single-file module (e.g. `foo.py` or `foo.pyi`)
    Module,

    /// A python package (`foo/__init__.py` or `foo/__init__.pyi`)
    Package,
}

impl ModuleKind {
    pub const fn is_package(self) -> bool {
        matches!(self, ModuleKind::Package)
    }
    pub const fn is_module(self) -> bool {
        matches!(self, ModuleKind::Module)
    }
}

/// Enumeration of various core stdlib modules in which important types are located
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum_macros::EnumString, get_size2::GetSize)]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
#[strum(serialize_all = "snake_case")]
pub enum KnownModule {
    Builtins,
    Enum,
    Types,
    #[strum(serialize = "_typeshed")]
    Typeshed,
    TypingExtensions,
    Typing,
    Sys,
    Os,
    Tempfile,
    Pathlib,
    Abc,
    Contextlib,
    Dataclasses,
    Collections,
    Inspect,
    #[strum(serialize = "string.templatelib")]
    Templatelib,
    #[strum(serialize = "_typeshed._type_checker_internals")]
    TypeCheckerInternals,
    TyExtensions,
    #[strum(serialize = "importlib")]
    ImportLib,
    #[strum(serialize = "unittest.mock")]
    UnittestMock,
    Uuid,
    Warnings,
}

impl KnownModule {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Builtins => "builtins",
            Self::Enum => "enum",
            Self::Types => "types",
            Self::Typing => "typing",
            Self::Typeshed => "_typeshed",
            Self::TypingExtensions => "typing_extensions",
            Self::Sys => "sys",
            Self::Os => "os",
            Self::Tempfile => "tempfile",
            Self::Pathlib => "pathlib",
            Self::Abc => "abc",
            Self::Contextlib => "contextlib",
            Self::Dataclasses => "dataclasses",
            Self::Collections => "collections",
            Self::Inspect => "inspect",
            Self::TypeCheckerInternals => "_typeshed._type_checker_internals",
            Self::TyExtensions => "ty_extensions",
            Self::ImportLib => "importlib",
            Self::Warnings => "warnings",
            Self::UnittestMock => "unittest.mock",
            Self::Uuid => "uuid",
            Self::Templatelib => "string.templatelib",
        }
    }

    pub fn name(self) -> ModuleName {
        ModuleName::new_static(self.as_str())
            .unwrap_or_else(|| panic!("{self} should be a valid module name!"))
    }

    fn try_from_search_path_and_name(search_path: &SearchPath, name: &ModuleName) -> Option<Self> {
        if search_path.is_standard_library() {
            Self::from_str(name.as_str()).ok()
        } else {
            None
        }
    }

    pub const fn is_builtins(self) -> bool {
        matches!(self, Self::Builtins)
    }

    pub const fn is_typing(self) -> bool {
        matches!(self, Self::Typing)
    }

    pub const fn is_ty_extensions(self) -> bool {
        matches!(self, Self::TyExtensions)
    }

    pub const fn is_inspect(self) -> bool {
        matches!(self, Self::Inspect)
    }

    pub const fn is_importlib(self) -> bool {
        matches!(self, Self::ImportLib)
    }
}

impl std::fmt::Display for KnownModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn known_module_roundtrip_from_str() {
        let stdlib_search_path = SearchPath::vendored_stdlib();

        for module in KnownModule::iter() {
            let module_name = module.name();

            assert_eq!(
                KnownModule::try_from_search_path_and_name(&stdlib_search_path, &module_name),
                Some(module),
                "The strum `EnumString` implementation appears to be incorrect for `{module_name}`"
            );
        }
    }
}
