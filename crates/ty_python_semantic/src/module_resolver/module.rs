use std::fmt::Formatter;
use std::str::FromStr;
use std::sync::Arc;

use ruff_db::files::File;

use super::path::SearchPath;
use crate::module_name::ModuleName;

/// Representation of a Python module.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Module {
    inner: Arc<ModuleInner>,
}

impl Module {
    pub(crate) fn file_module(
        name: ModuleName,
        kind: ModuleKind,
        search_path: SearchPath,
        file: File,
    ) -> Self {
        let known = KnownModule::try_from_search_path_and_name(&search_path, &name);

        Self {
            inner: Arc::new(ModuleInner::FileModule {
                name,
                kind,
                search_path,
                file,
                known,
            }),
        }
    }

    pub(crate) fn namespace_package(name: ModuleName) -> Self {
        Self {
            inner: Arc::new(ModuleInner::NamespacePackage { name }),
        }
    }

    /// The absolute name of the module (e.g. `foo.bar`)
    pub fn name(&self) -> &ModuleName {
        match &*self.inner {
            ModuleInner::FileModule { name, .. } => name,
            ModuleInner::NamespacePackage { name, .. } => name,
        }
    }

    /// The file to the source code that defines this module
    ///
    /// This is `None` for namespace packages.
    pub fn file(&self) -> Option<File> {
        match &*self.inner {
            ModuleInner::FileModule { file, .. } => Some(*file),
            ModuleInner::NamespacePackage { .. } => None,
        }
    }

    /// Is this a module that we special-case somehow? If so, which one?
    pub fn known(&self) -> Option<KnownModule> {
        match &*self.inner {
            ModuleInner::FileModule { known, .. } => *known,
            ModuleInner::NamespacePackage { .. } => None,
        }
    }

    /// Does this module represent the given known module?
    pub fn is_known(&self, known_module: KnownModule) -> bool {
        self.known() == Some(known_module)
    }

    /// The search path from which the module was resolved.
    pub(crate) fn search_path(&self) -> Option<&SearchPath> {
        match &*self.inner {
            ModuleInner::FileModule { search_path, .. } => Some(search_path),
            ModuleInner::NamespacePackage { .. } => None,
        }
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(&self) -> ModuleKind {
        match &*self.inner {
            ModuleInner::FileModule { kind, .. } => *kind,
            ModuleInner::NamespacePackage { .. } => ModuleKind::Package,
        }
    }
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name())
            .field("kind", &self.kind())
            .field("file", &self.file())
            .field("search_path", &self.search_path())
            .field("known", &self.known())
            .finish()
    }
}

#[derive(PartialEq, Eq, Hash)]
enum ModuleInner {
    /// A module that resolves to a file (`lib.py` or `package/__init__.py`)
    FileModule {
        name: ModuleName,
        kind: ModuleKind,
        search_path: SearchPath,
        file: File,
        known: Option<KnownModule>,
    },

    /// A namespace package. Namespace packages are special because
    /// there are multiple possible paths and they have no corresponding
    /// code file.
    NamespacePackage { name: ModuleName },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum_macros::EnumString)]
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
    Abc,
    Dataclasses,
    Collections,
    Inspect,
    #[strum(serialize = "_typeshed._type_checker_internals")]
    TypeCheckerInternals,
    TyExtensions,
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
            Self::Abc => "abc",
            Self::Dataclasses => "dataclasses",
            Self::Collections => "collections",
            Self::Inspect => "inspect",
            Self::TypeCheckerInternals => "_typeshed._type_checker_internals",
            Self::TyExtensions => "ty_extensions",
        }
    }

    pub fn name(self) -> ModuleName {
        ModuleName::new_static(self.as_str())
            .unwrap_or_else(|| panic!("{self} should be a valid module name!"))
    }

    pub(crate) fn try_from_search_path_and_name(
        search_path: &SearchPath,
        name: &ModuleName,
    ) -> Option<Self> {
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

    pub const fn is_enum(self) -> bool {
        matches!(self, Self::Enum)
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
