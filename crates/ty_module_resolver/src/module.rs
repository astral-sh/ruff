use std::borrow::Cow;
use std::fmt::Formatter;
use std::str::FromStr;

use ruff_db::files::File;
use ruff_python_ast::PythonVersion;
use salsa::Database;
use salsa::plumbing::AsId;

use crate::module_name::ModuleName;
use crate::path::SearchPath;
use crate::resolve::{
    self, ListingTarget, ModuleListing, ModuleNameIngredient, ModuleResolveMode,
    ModuleSearchCursor, ResolverContext, search_paths,
};
use crate::{Db, ResolverEnvironment};

/// Representation of a Python module.
#[derive(Clone, Copy, Eq, Hash, PartialEq, salsa::Supertype, salsa::SalsaValue)]
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
        file: File,
        resolver_environment: ResolverEnvironment<'db>,
        name: Cow<'_, ModuleName>,
        kind: ModuleKind,
        search_path: SearchPath,
    ) -> Self {
        let known = KnownModule::try_from_search_path_and_name(&search_path, &name);

        Self::File(FileModule::new(
            db,
            name,
            kind,
            search_path,
            file,
            resolver_environment,
            known,
        ))
    }

    pub(crate) fn namespace_package(
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
        name: Cow<'_, ModuleName>,
    ) -> Self {
        Self::Namespace(NamespacePackage::new(db, resolver_environment, name))
    }

    /// The resolver environment used to resolve this module.
    fn resolver_environment(self, db: &'db dyn Database) -> ResolverEnvironment<'db> {
        match self {
            Module::File(module) => module.resolver_environment(db),
            Module::Namespace(module) => module.resolver_environment(db),
        }
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

    /// The Python version used to resolve this module.
    pub fn python_version(self, db: &'db dyn Database) -> PythonVersion {
        self.resolver_environment(db).python_version(db)
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

    /// Returns whether this module resolves to a bundled typing-only stub.
    ///
    /// A project or installed module with the same name may still exist on a
    /// lower-priority search path and be available at runtime.
    pub fn is_type_check_only(self, db: &'db dyn Database) -> bool {
        self.search_path(db)
            .is_some_and(SearchPath::is_standard_library)
            && matches!(
                self.name(db).first_component(),
                "_typeshed" | "typing_extensions" | "ty_extensions"
            )
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(self, db: &'db dyn Database) -> ModuleKind {
        match self {
            Module::File(module) => module.kind(db),
            Module::Namespace(_) => ModuleKind::Package,
        }
    }

    /// Returns resolved immediate submodules, including portions of namespace packages.
    pub fn all_submodules(self, db: &'db dyn Db) -> &'db [Module<'db>] {
        &submodule_listing(db, self).modules
    }

    /// Returns the cached submodule listing needed for recursive module enumeration.
    /// Returns `None` when there are no modules or stub override prefixes to visit.
    pub(crate) fn submodule_listing(
        self,
        db: &'db dyn Db,
    ) -> Option<&'db CachedModuleListing<'db>> {
        let listing = submodule_listing(db, self);
        (!listing.is_empty()).then_some(listing)
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

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn submodule_listing<'db>(db: &'db dyn Db, module: Module<'db>) -> CachedModuleListing<'db> {
    let resolver_environment = module.resolver_environment(db);
    let context = ResolverContext::new(db, resolver_environment, ModuleResolveMode::Typing);

    // Desperate resolution can use a search path absent from the configuration.
    // Preserve that path when listing the module's submodules.
    if let Some(path) = module.search_path(db)
        && !search_paths(db, resolver_environment, ModuleResolveMode::Typing)
            .any(|configured| configured == path)
    {
        return ModuleSearchCursor::with_supplied_search_paths(
            &context,
            std::slice::from_ref(path),
        )
        .for_prefix(module.name(db))
        .map(|search| search.list_modules())
        .unwrap_or_default()
        .into();
    }

    resolve::list_modules(&context, &ListingTarget::ResolvedName(module)).into()
}

/// A cached listing of top-level modules or immediate submodules.
#[derive(Debug, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct CachedModuleListing<'db> {
    /// Resolved modules, suitable for import-statement completion.
    pub(crate) modules: Box<[Module<'db>]>,
    /// Prefixes used only by recursive enumeration; see [`ModuleListing`].
    stub_override_prefixes: Box<[ModuleName]>,
    /// Listed modules that may have descendants, including files with stub overrides.
    pub(crate) modules_with_possible_children: Box<[Module<'db>]>,
}

impl<'db> CachedModuleListing<'db> {
    /// Returns non-empty module listings for unresolved stub override prefixes.
    ///
    /// For example, suppose `/extra` is configured as an extra path and
    /// `acme-stubs` is a complete stub package (i.e. not marked as partial):
    ///
    /// ```text
    /// /extra/acme/nested/tools.pyi
    ///
    /// /site-packages/acme-stubs/__init__.pyi
    ///
    /// /site-packages/acme/__init__.py
    /// /site-packages/acme/nested/__init__.py
    /// /site-packages/acme/nested/tools.py
    /// ```
    ///
    /// Typing-mode resolution selects the installed stubs for `acme`. Those
    /// stubs do not supply `acme.nested`, and because the stub package is
    /// "complete" it prevents falling back to the source package. Nonetheless,
    /// the extra-path stub still supplies `acme.nested.tools`.
    ///
    /// Module enumeration must therefore visit the prefix `acme.nested` to reach
    /// `tools`, even though the prefix itself does not resolve to a module. This
    /// method returns listings beneath that prefix without including the prefix
    /// among the resolved modules.
    pub(crate) fn stub_override_listings(
        &self,
        db: &'db dyn Db,
        resolver_environment: ResolverEnvironment<'db>,
    ) -> impl Iterator<Item = &'db CachedModuleListing<'db>> {
        self.stub_override_prefixes
            .iter()
            .filter_map(move |prefix| {
                let name = ModuleNameIngredient::new(
                    db,
                    prefix,
                    ModuleResolveMode::Typing,
                    resolver_environment,
                );
                let listing = stub_override_listing(db, name);
                (!listing.is_empty()).then_some(listing)
            })
    }

    /// Whether there are no modules or stub override prefixes to visit during recursive enumeration.
    fn is_empty(&self) -> bool {
        self.modules.is_empty() && self.stub_override_prefixes.is_empty()
    }
}

impl<'db> From<ModuleListing<'db>> for CachedModuleListing<'db> {
    fn from(listing: ModuleListing<'db>) -> Self {
        Self {
            modules: listing.modules.into_boxed_slice(),
            stub_override_prefixes: listing.stub_override_prefixes.into_boxed_slice(),
            modules_with_possible_children: listing
                .modules_with_possible_children
                .into_boxed_slice(),
        }
    }
}

/// Cache each unresolved prefix by its name and environment, just as resolved
/// packages cache their submodule listings by `Module`.
#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn stub_override_listing<'db>(
    db: &'db dyn Db,
    name: ModuleNameIngredient<'db>,
) -> CachedModuleListing<'db> {
    let context = ResolverContext::new(db, name.resolver_environment(db), name.mode(db));
    resolve::list_modules(
        &context,
        &ListingTarget::UnresolvedName(name.name(db).clone()),
    )
    .into()
}

/// A module that resolves to a file (`lib.py` or `package/__init__.py`).
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct FileModule<'db> {
    #[returns(ref)]
    pub(super) name: ModuleName,
    #[returns(copy)]
    pub(super) kind: ModuleKind,
    #[returns(ref)]
    pub(super) search_path: SearchPath,
    #[returns(copy)]
    pub(super) file: File,
    #[returns(copy)]
    pub(super) resolver_environment: ResolverEnvironment<'db>,
    #[returns(copy)]
    pub(super) known: Option<KnownModule>,
}

/// A namespace package.
///
/// Namespace packages are special because there are
/// multiple possible paths and they have no corresponding code file.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct NamespacePackage<'db> {
    #[returns(copy)]
    pub(super) resolver_environment: ResolverEnvironment<'db>,
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

/// Enumeration of modules in which types with dedicated semantic behavior are located.
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
    Datetime,
    Decimal,
    Ipaddress,
    Re,
    Abc,
    Dataclasses,
    Functools,
    Collections,
    #[strum(serialize = "collections.abc")]
    CollectionsAbc,
    #[strum(serialize = "_collections_abc")]
    CollectionsAbcInternal,
    Inspect,
    #[strum(serialize = "string.templatelib")]
    Templatelib,
    #[strum(serialize = "_typeshed._type_checker_internals")]
    TypeCheckerInternals,
    TyExtensions,
    #[strum(serialize = "ty_extensions._internal")]
    TyExtensionsInternal,
    #[strum(serialize = "ty_extensions.pydantic")]
    TyExtensionsPydantic,
    #[strum(serialize = "importlib")]
    ImportLib,
    /// The standard-library `unittest.case` module.
    #[strum(serialize = "unittest.case")]
    UnittestCase,
    #[strum(serialize = "unittest.mock")]
    UnittestMock,
    Uuid,
    Warnings,
    Numbers,
    #[strum(serialize = "struct", serialize = "_struct")]
    Struct,
    // Third-party modules
    #[strum(serialize = "pydantic.config")]
    PydanticConfig,
    #[strum(serialize = "pydantic.fields")]
    PydanticFields,
    #[strum(serialize = "pydantic.functional_validators")]
    PydanticFunctionalValidators,
    #[strum(serialize = "pydantic.main")]
    PydanticMain,
    #[strum(serialize = "pydantic.root_model")]
    PydanticRootModel,
    #[strum(serialize = "pydantic_settings.main")]
    PydanticSettingsMain,
    #[strum(serialize = "pydantic.types")]
    PydanticTypes,
    Pytest,
    #[strum(serialize = "_pytest.config")]
    PytestConfig,
    #[strum(serialize = "_pytest.fixtures")]
    PytestFixtures,
    #[strum(serialize = "_pytest.mark.structures")]
    PytestMarkStructures,
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
            Self::Datetime => "datetime",
            Self::Decimal => "decimal",
            Self::Ipaddress => "ipaddress",
            Self::Re => "re",
            Self::Abc => "abc",
            Self::Dataclasses => "dataclasses",
            Self::Functools => "functools",
            Self::Collections => "collections",
            Self::CollectionsAbc => "collections.abc",
            Self::CollectionsAbcInternal => "_collections_abc",
            Self::Inspect => "inspect",
            Self::TypeCheckerInternals => "_typeshed._type_checker_internals",
            Self::TyExtensions => "ty_extensions",
            Self::TyExtensionsInternal => "ty_extensions._internal",
            Self::TyExtensionsPydantic => "ty_extensions.pydantic",
            Self::ImportLib => "importlib",
            Self::Warnings => "warnings",
            Self::UnittestCase => "unittest.case",
            Self::UnittestMock => "unittest.mock",
            Self::Uuid => "uuid",
            Self::Templatelib => "string.templatelib",
            Self::Numbers => "numbers",
            Self::Struct => "struct",
            Self::PydanticConfig => "pydantic.config",
            Self::PydanticFields => "pydantic.fields",
            Self::PydanticFunctionalValidators => "pydantic.functional_validators",
            Self::PydanticMain => "pydantic.main",
            Self::PydanticRootModel => "pydantic.root_model",
            Self::PydanticSettingsMain => "pydantic_settings.main",
            Self::PydanticTypes => "pydantic.types",
            Self::Pytest => "pytest",
            Self::PytestConfig => "_pytest.config",
            Self::PytestFixtures => "_pytest.fixtures",
            Self::PytestMarkStructures => "_pytest.mark.structures",
        }
    }

    pub fn name(self) -> ModuleName {
        ModuleName::new_static(self.as_str())
            .unwrap_or_else(|| panic!("{self} should be a valid module name!"))
    }

    fn try_from_search_path_and_name(search_path: &SearchPath, name: &ModuleName) -> Option<Self> {
        let known_module = Self::from_str(name.as_str()).ok()?;

        let is_expected_search_path = if known_module.is_third_party() {
            search_path.can_contain_third_party_code()
        } else {
            search_path.is_standard_library()
        };

        is_expected_search_path.then_some(known_module)
    }

    /// Return `true` if this module is provided by a supported third-party package.
    pub const fn is_third_party(self) -> bool {
        match self {
            Self::PydanticConfig
            | Self::PydanticFields
            | Self::PydanticFunctionalValidators
            | Self::PydanticMain
            | Self::PydanticRootModel
            | Self::PydanticSettingsMain
            | Self::PydanticTypes
            | Self::Pytest
            | Self::PytestConfig
            | Self::PytestFixtures
            | Self::PytestMarkStructures => true,
            Self::Builtins
            | Self::Enum
            | Self::Types
            | Self::Typeshed
            | Self::TypingExtensions
            | Self::Typing
            | Self::Sys
            | Self::Os
            | Self::Tempfile
            | Self::Pathlib
            | Self::Datetime
            | Self::Decimal
            | Self::Ipaddress
            | Self::Re
            | Self::Abc
            | Self::Dataclasses
            | Self::Functools
            | Self::Collections
            | Self::CollectionsAbc
            | Self::CollectionsAbcInternal
            | Self::Inspect
            | Self::Templatelib
            | Self::TypeCheckerInternals
            | Self::TyExtensions
            | Self::TyExtensionsInternal
            | Self::TyExtensionsPydantic
            | Self::ImportLib
            | Self::UnittestCase
            | Self::UnittestMock
            | Self::Uuid
            | Self::Warnings
            | Self::Numbers
            | Self::Struct => false,
        }
    }

    pub const fn is_builtins(self) -> bool {
        matches!(self, Self::Builtins)
    }

    pub const fn is_typing(self) -> bool {
        matches!(self, Self::Typing)
    }

    pub const fn is_typing_extensions(self) -> bool {
        matches!(self, Self::TypingExtensions)
    }

    pub const fn is_ty_extensions(self) -> bool {
        matches!(self, Self::TyExtensions)
    }

    pub const fn is_ty_extensions_internal(self) -> bool {
        matches!(self, Self::TyExtensionsInternal)
    }

    pub const fn is_inspect(self) -> bool {
        matches!(self, Self::Inspect)
    }

    pub const fn is_importlib(self) -> bool {
        matches!(self, Self::ImportLib)
    }

    pub const fn is_functools(self) -> bool {
        matches!(self, Self::Functools)
    }

    pub const fn is_dataclasses(self) -> bool {
        matches!(self, Self::Dataclasses)
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

        for module in KnownModule::iter().filter(|module| !module.is_third_party()) {
            let module_name = module.name();

            assert_eq!(
                KnownModule::try_from_search_path_and_name(&stdlib_search_path, &module_name),
                Some(module),
                "The strum `EnumString` implementation appears to be incorrect for `{module_name}`"
            );
        }
    }
}
