use ruff_macros::CacheKey;

/// A list of names imported via any import statement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, CacheKey)]
pub struct NameImports(Vec<NameImport>);

/// A representation of an individual name imported via any import statement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, CacheKey)]
pub enum NameImport {
    Import(ModuleNameImport),
    ImportFrom(MemberNameImport),
}

/// A representation of an individual name imported via an `import` statement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, CacheKey)]
pub struct ModuleNameImport {
    pub name: Alias,
}

/// A representation of an individual name imported via a `from ... import` statement.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, CacheKey)]
pub struct MemberNameImport {
    pub module: Option<String>,
    pub name: Alias,
    pub level: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, CacheKey)]
pub struct Alias {
    pub name: String,
    pub as_name: Option<String>,
}

impl NameImports {
    pub fn into_imports(self) -> Vec<NameImport> {
        self.0
    }
}

impl ModuleNameImport {
    /// Creates a new `Import` to import the specified module.
    pub fn module(name: String) -> Self {
        Self {
            name: Alias {
                name,
                as_name: None,
            },
        }
    }
}

impl MemberNameImport {
    /// Creates a new `ImportFrom` to import a member from the specified module.
    pub fn member(module: String, name: String) -> Self {
        Self {
            module: Some(module),
            name: Alias {
                name,
                as_name: None,
            },
            level: 0,
        }
    }

    pub fn alias(module: String, name: String, as_name: String) -> Self {
        Self {
            module: Some(module),
            name: Alias {
                name,
                as_name: Some(as_name),
            },
            level: 0,
        }
    }
}

impl std::fmt::Display for NameImport {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NameImport::Import(import) => write!(f, "{import}"),
            NameImport::ImportFrom(import_from) => write!(f, "{import_from}"),
        }
    }
}

impl std::fmt::Display for ModuleNameImport {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "import {}", self.name.name)?;
        if let Some(as_name) = self.name.as_name.as_ref() {
            write!(f, " as {as_name}")?;
        }
        Ok(())
    }
}

impl std::fmt::Display for MemberNameImport {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "from ")?;
        if self.level > 0 {
            write!(f, "{}", ".".repeat(self.level as usize))?;
        }
        if let Some(module) = self.module.as_ref() {
            write!(f, "{module}")?;
        }
        write!(f, " import {}", self.name.name)?;
        if let Some(as_name) = self.name.as_name.as_ref() {
            write!(f, " as {as_name}")?;
        }
        Ok(())
    }
}

pub trait FutureImport {
    /// Returns `true` if this import is from the `__future__` module.
    fn is_future_import(&self) -> bool;
}

impl FutureImport for ModuleNameImport {
    fn is_future_import(&self) -> bool {
        self.name.name == "__future__"
    }
}

impl FutureImport for MemberNameImport {
    fn is_future_import(&self) -> bool {
        self.module.as_deref() == Some("__future__")
    }
}

impl FutureImport for NameImport {
    fn is_future_import(&self) -> bool {
        match self {
            NameImport::Import(import) => import.is_future_import(),
            NameImport::ImportFrom(import_from) => import_from.is_future_import(),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for NameImports {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for NameImport {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            NameImport::Import(import) => serializer.collect_str(import),
            NameImport::ImportFrom(import_from) => serializer.collect_str(import_from),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::de::Deserialize<'de> for NameImports {
    fn deserialize<D: serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use ruff_python_ast::{self as ast, Stmt};
        use ruff_python_parser::Parsed;

        struct AnyNameImportsVisitor;

        impl<'de> serde::de::Visitor<'de> for AnyNameImportsVisitor {
            type Value = NameImports;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an import statement")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let body = ruff_python_parser::parse_module(value)
                    .map(Parsed::into_suite)
                    .map_err(E::custom)?;
                let [stmt] = body.as_slice() else {
                    return Err(E::custom("Expected a single statement"));
                };

                let imports = match stmt {
                    Stmt::ImportFrom(ast::StmtImportFrom {
                        module,
                        names,
                        level,
                        range: _,
                    }) => names
                        .iter()
                        .map(|name| {
                            NameImport::ImportFrom(MemberNameImport {
                                module: module.as_deref().map(ToString::to_string),
                                name: Alias {
                                    name: name.name.to_string(),
                                    as_name: name.asname.as_deref().map(ToString::to_string),
                                },
                                level: *level,
                            })
                        })
                        .collect(),
                    Stmt::Import(ast::StmtImport { names, range: _ }) => names
                        .iter()
                        .map(|name| {
                            NameImport::Import(ModuleNameImport {
                                name: Alias {
                                    name: name.name.to_string(),
                                    as_name: name.asname.as_deref().map(ToString::to_string),
                                },
                            })
                        })
                        .collect(),
                    _ => {
                        return Err(E::custom("Expected an import statement"));
                    }
                };

                Ok(NameImports(imports))
            }
        }

        deserializer.deserialize_str(AnyNameImportsVisitor)
    }
}

#[cfg(feature = "schemars")]
impl schemars::JsonSchema for NameImports {
    fn schema_name() -> String {
        "NameImports".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        schemars::schema::SchemaObject {
            instance_type: Some(schemars::schema::InstanceType::String.into()),
            ..Default::default()
        }
        .into()
    }
}
