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
    pub level: u32,
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
            level: 0,
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
        if self.level > 0 {
            write!(f, "{}", ".".repeat(self.level as usize))?;
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
