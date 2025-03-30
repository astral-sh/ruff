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
    pub(crate) fn new(
        name: ModuleName,
        kind: ModuleKind,
        search_path: SearchPath,
        file: File,
    ) -> Self {
        let known = KnownModule::try_from_search_path_and_name(&search_path, &name);
        Self {
            inner: Arc::new(ModuleInner {
                name,
                kind,
                search_path,
                file,
                known,
            }),
        }
    }

    /// The absolute name of the module (e.g. `foo.bar`)
    pub fn name(&self) -> &ModuleName {
        &self.inner.name
    }

    /// The file to the source code that defines this module
    pub fn file(&self) -> File {
        self.inner.file
    }

    /// Is this a module that we special-case somehow? If so, which one?
    pub fn known(&self) -> Option<KnownModule> {
        self.inner.known
    }

    /// Does this module represent the given known module?
    pub fn is_known(&self, known_module: KnownModule) -> bool {
        self.known() == Some(known_module)
    }

    /// The search path from which the module was resolved.
    pub(crate) fn search_path(&self) -> &SearchPath {
        &self.inner.search_path
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(&self) -> ModuleKind {
        self.inner.kind
    }
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name())
            .field("kind", &self.kind())
            .field("file", &self.file())
            .field("search_path", &self.search_path())
            .finish()
    }
}

#[derive(PartialEq, Eq, Hash)]
struct ModuleInner {
    name: ModuleName,
    kind: ModuleKind,
    search_path: SearchPath,
    file: File,
    known: Option<KnownModule>,
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
    #[allow(dead_code)]
    Abc, // currently only used in tests
    Collections,
    Inspect,
    KnotExtensions,
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
            Self::Collections => "collections",
            Self::Inspect => "inspect",
            Self::KnotExtensions => "knot_extensions",
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

    pub const fn is_knot_extensions(self) -> bool {
        matches!(self, Self::KnotExtensions)
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
