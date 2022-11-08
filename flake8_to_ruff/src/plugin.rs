use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;

use anyhow::anyhow;
use ruff::checks_gen::CheckCodePrefix;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum Plugin {
    Flake8Bugbear,
    Flake8Builtins,
    Flake8Comprehensions,
    Flake8Docstrings,
    Flake8Print,
    Flake8Quotes,
    Flake8Annotations,
    PEP8Naming,
    Pyupgrade,
}

impl FromStr for Plugin {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "flake8-bugbear" => Ok(Plugin::Flake8Bugbear),
            "flake8-builtins" => Ok(Plugin::Flake8Builtins),
            "flake8-comprehensions" => Ok(Plugin::Flake8Comprehensions),
            "flake8-docstrings" => Ok(Plugin::Flake8Docstrings),
            "flake8-print" => Ok(Plugin::Flake8Print),
            "flake8-quotes" => Ok(Plugin::Flake8Quotes),
            "flake8-annotations" => Ok(Plugin::Flake8Annotations),
            "pep8-naming" => Ok(Plugin::PEP8Naming),
            "pyupgrade" => Ok(Plugin::Pyupgrade),
            _ => Err(anyhow!("Unknown plugin: {}", string)),
        }
    }
}

impl Plugin {
    pub fn default(&self) -> CheckCodePrefix {
        match self {
            Plugin::Flake8Bugbear => CheckCodePrefix::B,
            Plugin::Flake8Builtins => CheckCodePrefix::A,
            Plugin::Flake8Comprehensions => CheckCodePrefix::C,
            Plugin::Flake8Docstrings => CheckCodePrefix::D,
            Plugin::Flake8Print => CheckCodePrefix::T,
            Plugin::Flake8Quotes => CheckCodePrefix::Q,
            Plugin::Flake8Annotations => CheckCodePrefix::ANN,
            Plugin::PEP8Naming => CheckCodePrefix::N,
            Plugin::Pyupgrade => CheckCodePrefix::U,
        }
    }

    pub fn select(&self, flake8: &HashMap<String, Option<String>>) -> Vec<CheckCodePrefix> {
        match self {
            Plugin::Flake8Bugbear => vec![CheckCodePrefix::B],
            Plugin::Flake8Builtins => vec![CheckCodePrefix::A],
            Plugin::Flake8Comprehensions => vec![CheckCodePrefix::C],
            Plugin::Flake8Docstrings => {
                // Use the user-provided docstring.
                for key in ["docstring-convention", "docstring_convention"] {
                    if let Some(Some(value)) = flake8.get(key) {
                        match DocstringConvention::from_str(value) {
                            Ok(convention) => return convention.select(),
                            Err(e) => {
                                eprintln!("Unexpected '{key}' value: {value} ({e}");
                                return vec![];
                            }
                        }
                    }
                }
                // Default to PEP8.
                DocstringConvention::PEP8.select()
            }
            Plugin::Flake8Print => vec![CheckCodePrefix::T],
            Plugin::Flake8Quotes => vec![CheckCodePrefix::Q],
            Plugin::Flake8Annotations => vec![CheckCodePrefix::ANN],
            Plugin::PEP8Naming => vec![CheckCodePrefix::N],
            Plugin::Pyupgrade => vec![CheckCodePrefix::U],
        }
    }
}

pub enum DocstringConvention {
    All,
    PEP8,
    NumPy,
    Google,
}

impl FromStr for DocstringConvention {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "all" => Ok(DocstringConvention::All),
            "pep8" => Ok(DocstringConvention::PEP8),
            "numpy" => Ok(DocstringConvention::NumPy),
            "google" => Ok(DocstringConvention::Google),
            _ => Err(anyhow!("Unknown docstring convention: {}", string)),
        }
    }
}

impl DocstringConvention {
    fn select(&self) -> Vec<CheckCodePrefix> {
        match self {
            DocstringConvention::All => vec![CheckCodePrefix::D],
            DocstringConvention::PEP8 => vec![
                // All errors except D203, D212, D213, D214, D215, D404, D405, D406, D407, D408,
                // D409, D410, D411, D413, D415, D416 and D417.
                CheckCodePrefix::D100,
                CheckCodePrefix::D101,
                CheckCodePrefix::D102,
                CheckCodePrefix::D103,
                CheckCodePrefix::D104,
                CheckCodePrefix::D105,
                CheckCodePrefix::D106,
                CheckCodePrefix::D107,
                CheckCodePrefix::D200,
                CheckCodePrefix::D201,
                CheckCodePrefix::D202,
                // CheckCodePrefix::D203,
                CheckCodePrefix::D204,
                CheckCodePrefix::D205,
                CheckCodePrefix::D206,
                CheckCodePrefix::D207,
                CheckCodePrefix::D208,
                CheckCodePrefix::D209,
                CheckCodePrefix::D210,
                CheckCodePrefix::D211,
                // CheckCodePrefix::D212,
                // CheckCodePrefix::D213,
                // CheckCodePrefix::D214,
                // CheckCodePrefix::D215,
                CheckCodePrefix::D300,
                CheckCodePrefix::D400,
                CheckCodePrefix::D402,
                CheckCodePrefix::D403,
                // CheckCodePrefix::D404,
                // CheckCodePrefix::D405,
                // CheckCodePrefix::D406,
                // CheckCodePrefix::D407,
                // CheckCodePrefix::D408,
                // CheckCodePrefix::D409,
                // CheckCodePrefix::D410,
                // CheckCodePrefix::D411,
                CheckCodePrefix::D412,
                // CheckCodePrefix::D413,
                CheckCodePrefix::D414,
                // CheckCodePrefix::D415,
                // CheckCodePrefix::D416,
                // CheckCodePrefix::D417,
                CheckCodePrefix::D418,
                CheckCodePrefix::D419,
            ],
            DocstringConvention::NumPy => vec![
                // All errors except D107, D203, D212, D213, D402, D413, D415, D416, and D417.
                CheckCodePrefix::D100,
                CheckCodePrefix::D101,
                CheckCodePrefix::D102,
                CheckCodePrefix::D103,
                CheckCodePrefix::D104,
                CheckCodePrefix::D105,
                CheckCodePrefix::D106,
                // CheckCodePrefix::D107,
                CheckCodePrefix::D200,
                CheckCodePrefix::D201,
                CheckCodePrefix::D202,
                // CheckCodePrefix::D203,
                CheckCodePrefix::D204,
                CheckCodePrefix::D205,
                CheckCodePrefix::D206,
                CheckCodePrefix::D207,
                CheckCodePrefix::D208,
                CheckCodePrefix::D209,
                CheckCodePrefix::D210,
                CheckCodePrefix::D211,
                // CheckCodePrefix::D212,
                // CheckCodePrefix::D213,
                CheckCodePrefix::D214,
                CheckCodePrefix::D215,
                CheckCodePrefix::D300,
                CheckCodePrefix::D400,
                // CheckCodePrefix::D402,
                CheckCodePrefix::D403,
                CheckCodePrefix::D404,
                CheckCodePrefix::D405,
                CheckCodePrefix::D406,
                CheckCodePrefix::D407,
                CheckCodePrefix::D408,
                CheckCodePrefix::D409,
                CheckCodePrefix::D410,
                CheckCodePrefix::D411,
                CheckCodePrefix::D412,
                // CheckCodePrefix::D413,
                CheckCodePrefix::D414,
                // CheckCodePrefix::D415,
                // CheckCodePrefix::D416,
                // CheckCodePrefix::D417,
                CheckCodePrefix::D418,
                CheckCodePrefix::D419,
            ],
            DocstringConvention::Google => vec![
                // All errors except D203, D204, D213, D215, D400, D401, D404, D406, D407, D408,
                // D409 and D413.
                CheckCodePrefix::D100,
                CheckCodePrefix::D101,
                CheckCodePrefix::D102,
                CheckCodePrefix::D103,
                CheckCodePrefix::D104,
                CheckCodePrefix::D105,
                CheckCodePrefix::D106,
                CheckCodePrefix::D107,
                CheckCodePrefix::D200,
                CheckCodePrefix::D201,
                CheckCodePrefix::D202,
                // CheckCodePrefix::D203,
                // CheckCodePrefix::D204,
                CheckCodePrefix::D205,
                CheckCodePrefix::D206,
                CheckCodePrefix::D207,
                CheckCodePrefix::D208,
                CheckCodePrefix::D209,
                CheckCodePrefix::D210,
                CheckCodePrefix::D211,
                CheckCodePrefix::D212,
                // CheckCodePrefix::D213,
                CheckCodePrefix::D214,
                // CheckCodePrefix::D215,
                CheckCodePrefix::D300,
                // CheckCodePrefix::D400,
                CheckCodePrefix::D402,
                CheckCodePrefix::D403,
                // CheckCodePrefix::D404,
                CheckCodePrefix::D405,
                // CheckCodePrefix::D406,
                // CheckCodePrefix::D407,
                // CheckCodePrefix::D408,
                // CheckCodePrefix::D409,
                CheckCodePrefix::D410,
                CheckCodePrefix::D411,
                CheckCodePrefix::D412,
                // CheckCodePrefix::D413,
                CheckCodePrefix::D414,
                CheckCodePrefix::D415,
                CheckCodePrefix::D416,
                CheckCodePrefix::D417,
                CheckCodePrefix::D418,
                CheckCodePrefix::D419,
            ],
        }
    }
}

/// Infer the enabled plugins based on user-provided options.
///
/// For example, if the user specified a `mypy-init-return` setting, we should
/// infer that `flake8-annotations` is active.
pub fn infer_plugins_from_options(flake8: &HashMap<String, Option<String>>) -> Vec<Plugin> {
    let mut plugins = BTreeSet::new();
    for key in flake8.keys() {
        match key.as_str() {
            // flake8-docstrings
            "docstring-convention" | "docstring_convention" => {
                plugins.insert(Plugin::Flake8Docstrings);
            }
            // flake8-builtins
            "builtins-ignorelist" | "builtins_ignorelist" => {
                plugins.insert(Plugin::Flake8Builtins);
            }
            // flake8-quotes
            "quotes" | "inline-quotes" | "inline_quotes" => {
                plugins.insert(Plugin::Flake8Quotes);
            }
            "multiline-quotes" | "multiline_quotes" => {
                plugins.insert(Plugin::Flake8Quotes);
            }
            "docstring-quotes" | "docstring_quotes" => {
                plugins.insert(Plugin::Flake8Quotes);
            }
            "avoid-escape" | "avoid_escape" => {
                plugins.insert(Plugin::Flake8Quotes);
            }
            // flake8-annotations
            "suppress-none-returning" | "suppress_none_returning" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "suppress-dummy-args" | "suppress_dummy_args" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "allow-untyped-defs" | "allow_untyped_defs" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "allow-untyped-nested" | "allow_untyped_nested" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "mypy-init-return" | "mypy_init_return" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "dispatch-decorators" | "dispatch_decorators" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "overload-decorators" | "overload_decorators" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            "allow-star-arg-any" | "allow_star_arg_any" => {
                plugins.insert(Plugin::Flake8Annotations);
            }
            // pep8-naming
            "ignore-names" | "ignore_names" => {
                plugins.insert(Plugin::PEP8Naming);
            }
            "classmethod-decorators" | "classmethod_decorators" => {
                plugins.insert(Plugin::PEP8Naming);
            }
            "staticmethod-decorators" | "staticmethod_decorators" => {
                plugins.insert(Plugin::PEP8Naming);
            }
            _ => {}
        }
    }
    Vec::from_iter(plugins)
}

/// Infer the enabled plugins based on the referenced prefixes.
///
/// For example, if the user ignores `ANN101`, we should infer that
/// `flake8-annotations` is active.
pub fn infer_plugins_from_codes(codes: &BTreeSet<CheckCodePrefix>) -> Vec<Plugin> {
    [
        Plugin::Flake8Bugbear,
        Plugin::Flake8Builtins,
        Plugin::Flake8Comprehensions,
        Plugin::Flake8Docstrings,
        Plugin::Flake8Print,
        Plugin::Flake8Quotes,
        Plugin::Flake8Annotations,
        Plugin::PEP8Naming,
        Plugin::Pyupgrade,
    ]
    .into_iter()
    .filter(|plugin| {
        for prefix in codes {
            if prefix
                .codes()
                .iter()
                .any(|code| plugin.default().codes().contains(code))
            {
                return true;
            }
        }
        false
    })
    .collect()
}

/// Resolve the set of enabled `CheckCodePrefix` values for the given plugins.
pub fn resolve_select(
    flake8: &HashMap<String, Option<String>>,
    plugins: &[Plugin],
) -> BTreeSet<CheckCodePrefix> {
    // Include default Pyflakes and pycodestyle checks.
    let mut select = BTreeSet::from([CheckCodePrefix::E, CheckCodePrefix::F, CheckCodePrefix::W]);

    // Add prefix codes for every plugin.
    for plugin in plugins {
        select.extend(plugin.select(flake8));
    }

    select
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::plugin::{infer_plugins_from_options, Plugin};

    #[test]
    fn it_infers_plugins() {
        let actual = infer_plugins_from_options(&HashMap::from([(
            "inline-quotes".to_string(),
            Some("single".to_string()),
        )]));
        let expected = vec![Plugin::Flake8Quotes];
        assert_eq!(actual, expected);

        let actual = infer_plugins_from_options(&HashMap::from([(
            "staticmethod-decorators".to_string(),
            Some("[]".to_string()),
        )]));
        let expected = vec![Plugin::PEP8Naming];
        assert_eq!(actual, expected);
    }
}
