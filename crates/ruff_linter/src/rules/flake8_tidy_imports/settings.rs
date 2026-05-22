use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::display_settings;
use crate::rules::flake8_tidy_imports::matchers::NameMatchPolicy;
use ruff_macros::CacheKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ApiBan {
    /// The message to display when the API is used.
    pub msg: String,
}

impl Display for ApiBan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    #[default]
    Parents,
    /// Ban all relative imports.
    All,
}

impl Display for Strictness {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parents => write!(f, "\"parents\""),
            Self::All => write!(f, "\"all\""),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AllImports {
    All,
}

impl Display for AllImports {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "\"all\""),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(untagged)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ImportSelection {
    All(AllImports),
    Imports(Vec<String>),
}

impl Default for ImportSelection {
    fn default() -> Self {
        Self::Imports(Vec::new())
    }
}

fn fmt_imports(f: &mut Formatter<'_>, imports: &[String], indent: &str) -> std::fmt::Result {
    if imports.is_empty() {
        write!(f, "[]")
    } else {
        writeln!(f, "[")?;
        for import in imports {
            writeln!(f, "{indent}{import},")?;
        }
        write!(f, "]")
    }
}

impl Display for ImportSelection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All(all) => write!(f, "{all}"),
            Self::Imports(imports) => fmt_imports(f, imports, "\t"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ImportSelectorSettings {
    pub include: ImportSelection,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Display for ImportSelectorSettings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{")?;
        write!(f, "\tinclude = ")?;
        match &self.include {
            ImportSelection::All(all) => writeln!(f, "{all}")?,
            ImportSelection::Imports(imports) => {
                fmt_imports(f, imports, "\t\t")?;
                writeln!(f)?;
            }
        }
        write!(f, "\texclude = ")?;
        fmt_imports(f, &self.exclude, "\t\t")?;
        write!(f, "\n}}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(untagged)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ImportSelector {
    Selection(ImportSelection),
    Settings(ImportSelectorSettings),
}

impl Default for ImportSelector {
    fn default() -> Self {
        Self::Selection(ImportSelection::default())
    }
}

impl ImportSelector {
    pub fn include(&self) -> &ImportSelection {
        match self {
            Self::Selection(selection) => selection,
            Self::Settings(settings) => &settings.include,
        }
    }

    pub fn exclude(&self) -> &[String] {
        match self {
            Self::Selection(_) => &[],
            Self::Settings(settings) => &settings.exclude,
        }
    }

    pub fn includes_all(&self) -> bool {
        matches!(self.include(), ImportSelection::All(AllImports::All))
    }

    pub(crate) fn find(&self, policy: &NameMatchPolicy) -> Option<ImportMatch> {
        if policy
            .find(self.exclude().iter().map(String::as_str))
            .is_some()
        {
            return None;
        }

        match self.include() {
            ImportSelection::All(AllImports::All) => Some(ImportMatch::All),
            ImportSelection::Imports(imports) => policy
                .find(imports.iter().map(String::as_str))
                .map(ImportMatch::Named),
        }
    }
}

pub(crate) enum ImportMatch {
    /// Matched all imports (no specific name).
    All,
    /// Matched a specific import by name.
    Named(String),
}

impl ImportMatch {
    pub(crate) fn name(self) -> Option<String> {
        match self {
            ImportMatch::All => None,
            ImportMatch::Named(name) => Some(name),
        }
    }
}

impl Display for ImportSelector {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Selection(selection) => write!(f, "{selection}"),
            Self::Settings(settings) => write!(f, "{settings}"),
        }
    }
}

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub ban_relative_imports: Strictness,
    pub banned_api: FxHashMap<String, ApiBan>,
    pub banned_module_level_imports: Vec<String>,
    pub require_lazy: ImportSelector,
    pub ban_lazy: ImportSelector,
}

impl Settings {
    pub fn banned_module_level_imports(&self) -> impl Iterator<Item = &str> {
        self.banned_module_level_imports.iter().map(AsRef::as_ref)
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_tidy_imports",
            fields = [
                self.ban_relative_imports,
                self.banned_api | map,
                self.banned_module_level_imports | array,
                self.require_lazy,
                self.ban_lazy,
            ]
        }
        Ok(())
    }
}
