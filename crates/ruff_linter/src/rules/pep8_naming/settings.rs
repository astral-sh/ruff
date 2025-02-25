//! Settings for the `pep8-naming` plugin.

use std::error::Error;
use std::fmt;
use std::fmt::Formatter;

use globset::{Glob, GlobSet, GlobSetBuilder};

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_macros::CacheKey;

use crate::display_settings;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub ignore_names: IgnoreNames,
    pub classmethod_decorators: Vec<String>,
    pub staticmethod_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IgnoreNames::Default,
            classmethod_decorators: Vec::new(),
            staticmethod_decorators: Vec::new(),
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pep8_naming",
            fields = [
                self.ignore_names,
                self.classmethod_decorators | array,
                self.staticmethod_decorators | array
            ]
        }
        Ok(())
    }
}

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidIgnoreName(globset::Error),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::InvalidIgnoreName(err) => {
                write!(f, "Invalid pattern in ignore-names: {err}")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidIgnoreName(err) => Some(err),
        }
    }
}

/// The default names to ignore.
///
/// Must be kept in-sync with the `matches!` macro in [`IgnoreNames::matches`].
static DEFAULTS: &[&str] = &[
    "setUp",
    "tearDown",
    "setUpClass",
    "tearDownClass",
    "setUpModule",
    "tearDownModule",
    "asyncSetUp",
    "asyncTearDown",
    "setUpTestData",
    "failureException",
    "longMessage",
    "maxDiff",
];

#[derive(Debug, Clone)]
pub enum IgnoreNames {
    Default,
    UserProvided {
        matcher: GlobSet,
        literals: Vec<String>,
    },
}

impl IgnoreNames {
    /// Create a new [`IgnoreNames`] from the given options.
    pub fn from_options(
        ignore_names: Option<Vec<String>>,
        extend_ignore_names: Option<Vec<String>>,
    ) -> Result<Self, SettingsError> {
        // If the user is not customizing the set of ignored names, use the default matcher,
        // which is hard-coded to avoid expensive regex matching.
        if ignore_names.is_none() && extend_ignore_names.as_ref().is_none_or(Vec::is_empty) {
            return Ok(IgnoreNames::Default);
        }

        let mut builder = GlobSetBuilder::new();
        let mut literals = Vec::new();

        // Add the ignored names from the `ignore-names` option. If omitted entirely, use the
        // defaults
        if let Some(names) = ignore_names {
            for name in names {
                builder.add(Glob::new(&name).map_err(SettingsError::InvalidIgnoreName)?);
                literals.push(name);
            }
        } else {
            for name in DEFAULTS {
                builder.add(Glob::new(name).unwrap());
                literals.push((*name).to_string());
            }
        }

        // Add the ignored names from the `extend-ignore-names` option.
        if let Some(names) = extend_ignore_names {
            for name in names {
                builder.add(Glob::new(&name).map_err(SettingsError::InvalidIgnoreName)?);
                literals.push(name);
            }
        }

        let matcher = builder.build().map_err(SettingsError::InvalidIgnoreName)?;

        Ok(IgnoreNames::UserProvided { matcher, literals })
    }

    /// Returns `true` if the given name matches any of the ignored patterns.
    pub fn matches(&self, name: &str) -> bool {
        match self {
            IgnoreNames::Default => matches!(
                name,
                "setUp"
                    | "tearDown"
                    | "setUpClass"
                    | "tearDownClass"
                    | "setUpModule"
                    | "tearDownModule"
                    | "asyncSetUp"
                    | "asyncTearDown"
                    | "setUpTestData"
                    | "failureException"
                    | "longMessage"
                    | "maxDiff"
            ),
            IgnoreNames::UserProvided { matcher, .. } => matcher.is_match(name),
        }
    }

    /// Create a new [`IgnoreNames`] from the given patterns.
    pub fn from_patterns(
        patterns: impl IntoIterator<Item = String>,
    ) -> Result<Self, SettingsError> {
        let mut builder = GlobSetBuilder::new();
        let mut literals = Vec::new();

        for name in patterns {
            builder.add(Glob::new(&name).map_err(SettingsError::InvalidIgnoreName)?);
            literals.push(name);
        }

        let matcher = builder.build().map_err(SettingsError::InvalidIgnoreName)?;

        Ok(IgnoreNames::UserProvided { matcher, literals })
    }
}

impl CacheKey for IgnoreNames {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        match self {
            IgnoreNames::Default => {
                "default".cache_key(state);
            }
            IgnoreNames::UserProvided {
                literals: patterns, ..
            } => {
                "user-provided".cache_key(state);
                patterns.cache_key(state);
            }
        }
    }
}

impl fmt::Display for IgnoreNames {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IgnoreNames::Default => {
                writeln!(f, "[")?;
                for elem in DEFAULTS {
                    writeln!(f, "\t{elem},")?;
                }
                write!(f, "]")?;
            }
            IgnoreNames::UserProvided {
                literals: patterns, ..
            } => {
                writeln!(f, "[")?;
                for elem in patterns {
                    writeln!(f, "\t{elem},")?;
                }
                write!(f, "]")?;
            }
        }
        Ok(())
    }
}
