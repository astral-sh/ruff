use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::str::FromStr;

use anyhow::anyhow;
use ruff::registry_gen::CheckCodePrefix;

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum Plugin {
    Flake8Annotations,
    Flake8Bandit,
    Flake8BlindExcept,
    Flake8Bugbear,
    Flake8Builtins,
    Flake8Comprehensions,
    Flake8Datetimez,
    Flake8Debugger,
    Flake8Docstrings,
    Flake8Eradicate,
    Flake8ErrMsg,
    Flake8ImplicitStrConcat,
    Flake8Print,
    Flake8PytestStyle,
    Flake8Quotes,
    Flake8Return,
    Flake8Simplify,
    Flake8TidyImports,
    McCabe,
    PEP8Naming,
    PandasVet,
    Pyupgrade,
}

impl FromStr for Plugin {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "flake8-annotations" => Ok(Plugin::Flake8Annotations),
            "flake8-bandit" => Ok(Plugin::Flake8Bandit),
            "flake8-blind-except" => Ok(Plugin::Flake8BlindExcept),
            "flake8-bugbear" => Ok(Plugin::Flake8Bugbear),
            "flake8-builtins" => Ok(Plugin::Flake8Builtins),
            "flake8-comprehensions" => Ok(Plugin::Flake8Comprehensions),
            "flake8-datetimez" => Ok(Plugin::Flake8Datetimez),
            "flake8-debugger" => Ok(Plugin::Flake8Debugger),
            "flake8-docstrings" => Ok(Plugin::Flake8Docstrings),
            "flake8-eradicate" => Ok(Plugin::Flake8Eradicate),
            "flake8-errmsg" => Ok(Plugin::Flake8ErrMsg),
            "flake8-implicit-str-concat" => Ok(Plugin::Flake8ImplicitStrConcat),
            "flake8-print" => Ok(Plugin::Flake8Print),
            "flake8-pytest-style" => Ok(Plugin::Flake8PytestStyle),
            "flake8-quotes" => Ok(Plugin::Flake8Quotes),
            "flake8-return" => Ok(Plugin::Flake8Return),
            "flake8-simplify" => Ok(Plugin::Flake8Simplify),
            "flake8-tidy-imports" => Ok(Plugin::Flake8TidyImports),
            "mccabe" => Ok(Plugin::McCabe),
            "pandas-vet" => Ok(Plugin::PandasVet),
            "pep8-naming" => Ok(Plugin::PEP8Naming),
            "pyupgrade" => Ok(Plugin::Pyupgrade),
            _ => Err(anyhow!("Unknown plugin: {string}")),
        }
    }
}

impl fmt::Debug for Plugin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Plugin::Flake8Annotations => "flake8-annotations",
                Plugin::Flake8Bandit => "flake8-bandit",
                Plugin::Flake8BlindExcept => "flake8-blind-except",
                Plugin::Flake8Bugbear => "flake8-bugbear",
                Plugin::Flake8Builtins => "flake8-builtins",
                Plugin::Flake8Comprehensions => "flake8-comprehensions",
                Plugin::Flake8Datetimez => "flake8-datetimez",
                Plugin::Flake8Debugger => "flake8-debugger",
                Plugin::Flake8Docstrings => "flake8-docstrings",
                Plugin::Flake8Eradicate => "flake8-eradicate",
                Plugin::Flake8ErrMsg => "flake8-errmsg",
                Plugin::Flake8ImplicitStrConcat => "flake8-implicit-str-concat",
                Plugin::Flake8Print => "flake8-print",
                Plugin::Flake8PytestStyle => "flake8-pytest-style",
                Plugin::Flake8Quotes => "flake8-quotes",
                Plugin::Flake8Return => "flake8-return",
                Plugin::Flake8Simplify => "flake8-simplify",
                Plugin::Flake8TidyImports => "flake8-tidy-imports",
                Plugin::McCabe => "mccabe",
                Plugin::PEP8Naming => "pep8-naming",
                Plugin::PandasVet => "pandas-vet",
                Plugin::Pyupgrade => "pyupgrade",
            }
        )
    }
}

impl Plugin {
    pub fn prefix(&self) -> CheckCodePrefix {
        match self {
            Plugin::Flake8Annotations => CheckCodePrefix::ANN,
            Plugin::Flake8Bandit => CheckCodePrefix::S,
            // TODO(charlie): Handle rename of `B` to `BLE`.
            Plugin::Flake8BlindExcept => CheckCodePrefix::BLE,
            Plugin::Flake8Bugbear => CheckCodePrefix::B,
            Plugin::Flake8Builtins => CheckCodePrefix::A,
            Plugin::Flake8Comprehensions => CheckCodePrefix::C4,
            Plugin::Flake8Datetimez => CheckCodePrefix::DTZ,
            Plugin::Flake8Debugger => CheckCodePrefix::T1,
            Plugin::Flake8Docstrings => CheckCodePrefix::D,
            // TODO(charlie): Handle rename of `E` to `ERA`.
            Plugin::Flake8Eradicate => CheckCodePrefix::ERA,
            Plugin::Flake8ErrMsg => CheckCodePrefix::EM,
            Plugin::Flake8ImplicitStrConcat => CheckCodePrefix::ISC,
            Plugin::Flake8Print => CheckCodePrefix::T2,
            Plugin::Flake8PytestStyle => CheckCodePrefix::PT,
            Plugin::Flake8Quotes => CheckCodePrefix::Q,
            Plugin::Flake8Return => CheckCodePrefix::RET,
            Plugin::Flake8Simplify => CheckCodePrefix::SIM,
            Plugin::Flake8TidyImports => CheckCodePrefix::TID25,
            Plugin::McCabe => CheckCodePrefix::C9,
            Plugin::PandasVet => CheckCodePrefix::PD,
            Plugin::PEP8Naming => CheckCodePrefix::N,
            Plugin::Pyupgrade => CheckCodePrefix::UP,
        }
    }
}

pub enum DocstringConvention {
    All,
    Pep257,
    Numpy,
    Google,
}

impl FromStr for DocstringConvention {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "all" => Ok(DocstringConvention::All),
            "pep257" => Ok(DocstringConvention::Pep257),
            "numpy" => Ok(DocstringConvention::Numpy),
            "google" => Ok(DocstringConvention::Google),
            _ => Err(anyhow!("Unknown docstring convention: {string}")),
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
            // flake8-bugbear
            "extend-immutable-calls" | "extend_immutable_calls" => {
                plugins.insert(Plugin::Flake8Bugbear);
            }
            // flake8-builtins
            "builtins-ignorelist" | "builtins_ignorelist" => {
                plugins.insert(Plugin::Flake8Builtins);
            }
            // flake8-docstrings
            "docstring-convention" | "docstring_convention" => {
                plugins.insert(Plugin::Flake8Docstrings);
            }
            // flake8-eradicate
            "eradicate-aggressive" | "eradicate_aggressive" => {
                plugins.insert(Plugin::Flake8Eradicate);
            }
            "eradicate-whitelist" | "eradicate_whitelist" => {
                plugins.insert(Plugin::Flake8Eradicate);
            }
            "eradicate-whitelist-extend" | "eradicate_whitelist_extend" => {
                plugins.insert(Plugin::Flake8Eradicate);
            }
            // flake8-pytest-style
            "pytest-fixture-no-parentheses" | "pytest_fixture_no_parentheses " => {
                plugins.insert(Plugin::Flake8PytestStyle);
            }
            "pytest-parametrize-names-type" | "pytest_parametrize_names_type" => {
                plugins.insert(Plugin::Flake8PytestStyle);
            }
            "pytest-parametrize-values-type" | "pytest_parametrize_values_type" => {
                plugins.insert(Plugin::Flake8PytestStyle);
            }
            "pytest-parametrize-values-row-type" | "pytest_parametrize_values_row_type" => {
                plugins.insert(Plugin::Flake8PytestStyle);
            }
            "pytest-raises-require-match-for" | "pytest_raises_require_match_for" => {
                plugins.insert(Plugin::Flake8PytestStyle);
            }
            "pytest-mark-no-parentheses" | "pytest_mark_no_parentheses" => {
                plugins.insert(Plugin::Flake8PytestStyle);
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
            // flake8-tidy-imports
            "ban-relative-imports" | "ban_relative_imports" => {
                plugins.insert(Plugin::Flake8TidyImports);
            }
            "banned-modules" | "banned_modules" => {
                plugins.insert(Plugin::Flake8TidyImports);
            }
            // mccabe
            "max-complexity" | "max_complexity" => {
                plugins.insert(Plugin::McCabe);
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
            "max-string-length" | "max_string_length" => {
                plugins.insert(Plugin::Flake8ErrMsg);
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
        Plugin::Flake8Annotations,
        Plugin::Flake8Bandit,
        Plugin::Flake8BlindExcept,
        Plugin::Flake8Bugbear,
        Plugin::Flake8Builtins,
        Plugin::Flake8Comprehensions,
        Plugin::Flake8Datetimez,
        Plugin::Flake8Debugger,
        Plugin::Flake8Docstrings,
        Plugin::Flake8Eradicate,
        Plugin::Flake8ErrMsg,
        Plugin::Flake8ImplicitStrConcat,
        Plugin::Flake8Print,
        Plugin::Flake8Quotes,
        Plugin::Flake8Return,
        Plugin::Flake8Simplify,
        Plugin::Flake8TidyImports,
        Plugin::PandasVet,
        Plugin::PEP8Naming,
    ]
    .into_iter()
    .filter(|plugin| {
        for prefix in codes {
            if prefix
                .codes()
                .iter()
                .any(|code| plugin.prefix().codes().contains(code))
            {
                return true;
            }
        }
        false
    })
    .collect()
}

/// Resolve the set of enabled `CheckCodePrefix` values for the given plugins.
pub fn resolve_select(plugins: &[Plugin]) -> BTreeSet<CheckCodePrefix> {
    let mut select = BTreeSet::from([CheckCodePrefix::F, CheckCodePrefix::E, CheckCodePrefix::W]);
    select.extend(plugins.iter().map(Plugin::prefix));
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
