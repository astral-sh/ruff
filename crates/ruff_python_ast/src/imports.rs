use std::fmt;

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
    pub level: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias<'a> {
    pub name: &'a str,
    pub as_name: Option<&'a str>,
}

impl<'a> Import<'a> {
    pub fn module(name: &'a str) -> Self {
        Self {
            name: Alias {
                name,
                as_name: None,
            },
        }
    }
}

impl fmt::Display for AnyImport<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AnyImport::Import(import) => write!(f, "{import}"),
            AnyImport::ImportFrom(import_from) => write!(f, "{import_from}"),
        }
    }
}

impl fmt::Display for Import<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "import {}", self.name.name)?;
        if let Some(as_name) = self.name.as_name {
            write!(f, " as {as_name}")?;
        }
        Ok(())
    }
}

impl fmt::Display for ImportFrom<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "from ")?;
        if let Some(level) = self.level {
            write!(f, "{}", ".".repeat(level))?;
        }
        if let Some(module) = self.module {
            write!(f, "{module}")?;
        }
        write!(f, " import {}", self.name.name)?;
        Ok(())
    }
}
