use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A representation of an individual name imported via any import statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnyImport<'a> {
    Import(Import<'a>),
    ImportFrom(ImportFrom<'a>),
}

/// A representation of an individual name imported via an `import` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import<'a> {
    pub name: Alias<'a>,
}

/// A representation of an individual name imported via a `from ... import` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFrom<'a> {
    pub module: Option<&'a str>,
    pub name: Alias<'a>,
    pub level: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias<'a> {
    pub name: &'a str,
    pub as_name: Option<&'a str>,
}

impl<'a> Import<'a> {
    /// Creates a new `Import` to import the specified module.
    pub fn module(name: &'a str) -> Self {
        Self {
            name: Alias {
                name,
                as_name: None,
            },
        }
    }
}

impl<'a> ImportFrom<'a> {
    /// Creates a new `ImportFrom` to import a member from the specified module.
    pub fn member(module: &'a str, name: &'a str) -> Self {
        Self {
            module: Some(module),
            name: Alias {
                name,
                as_name: None,
            },
            level: None,
        }
    }
}

impl std::fmt::Display for AnyImport<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AnyImport::Import(import) => write!(f, "{import}"),
            AnyImport::ImportFrom(import_from) => write!(f, "{import_from}"),
        }
    }
}

impl std::fmt::Display for Import<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "import {}", self.name.name)?;
        if let Some(as_name) = self.name.as_name {
            write!(f, " as {as_name}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for ImportFrom<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "from ")?;
        if let Some(level) = self.level {
            write!(f, "{}", ".".repeat(level as usize))?;
        }
        if let Some(module) = self.module {
            write!(f, "{module}")?;
        }
        write!(f, " import {}", self.name.name)?;
        if let Some(as_name) = self.name.as_name {
            write!(f, " as {as_name}")?;
        }
        Ok(())
    }
}

pub trait FutureImport {
    /// Returns `true` if this import is from the `__future__` module.
    fn is_future_import(&self) -> bool;
}

impl FutureImport for Import<'_> {
    fn is_future_import(&self) -> bool {
        self.name.name == "__future__"
    }
}

impl FutureImport for ImportFrom<'_> {
    fn is_future_import(&self) -> bool {
        self.module == Some("__future__")
    }
}

impl FutureImport for AnyImport<'_> {
    fn is_future_import(&self) -> bool {
        match self {
            AnyImport::Import(import) => import.is_future_import(),
            AnyImport::ImportFrom(import_from) => import_from.is_future_import(),
        }
    }
}

/// A representation of a module reference in an import statement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ModuleImport {
    module: String,
    range: TextRange,
}

impl ModuleImport {
    pub fn new(module: String, range: TextRange) -> Self {
        Self { module, range }
    }
}

impl From<&ModuleImport> for TextRange {
    fn from(import: &ModuleImport) -> TextRange {
        import.range
    }
}

/// A representation of the import dependencies between modules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImportMap {
    /// A map from dot-delimited module name to the list of imports in that module.
    module_to_imports: FxHashMap<String, Vec<ModuleImport>>,
}

impl ImportMap {
    pub fn new() -> Self {
        Self {
            module_to_imports: FxHashMap::default(),
        }
    }

    pub fn insert(&mut self, module: String, imports_vec: Vec<ModuleImport>) {
        self.module_to_imports.insert(module, imports_vec);
    }

    pub fn extend(&mut self, other: Self) {
        self.module_to_imports.extend(other.module_to_imports);
    }
}

impl<'a> IntoIterator for &'a ImportMap {
    type IntoIter = std::collections::hash_map::Iter<'a, String, Vec<ModuleImport>>;
    type Item = (&'a String, &'a Vec<ModuleImport>);

    fn into_iter(self) -> Self::IntoIter {
        self.module_to_imports.iter()
    }
}
