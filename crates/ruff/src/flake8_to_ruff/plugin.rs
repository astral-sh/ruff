use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use anyhow::anyhow;

use crate::registry::RuleCodePrefix;
use crate::rule_selector::RuleSelector;

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum Plugin {
    Flake82020,
    Flake8Annotations,
    Flake8Bandit,
    Flake8BlindExcept,
    Flake8BooleanTrap,
    Flake8Bugbear,
    Flake8Builtins,
    Flake8Commas,
    Flake8Comprehensions,
    Flake8Datetimez,
    Flake8Debugger,
    Flake8Docstrings,
    Flake8Eradicate,
    Flake8ErrMsg,
    Flake8Executable,
    Flake8ImplicitStrConcat,
    Flake8ImportConventions,
    Flake8NoPep420,
    Flake8Pie,
    Flake8Print,
    Flake8PytestStyle,
    Flake8Quotes,
    Flake8Return,
    Flake8Simplify,
    Flake8TidyImports,
    Flake8TypeChecking,
    Flake8UnusedArguments,
    Flake8UsePathlib,
    McCabe,
    PEP8Naming,
    PandasVet,
    Pyupgrade,
    Tryceratops,
}

impl FromStr for Plugin {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "flake8-2020" => Ok(Plugin::Flake82020),
            "flake8-annotations" => Ok(Plugin::Flake8Annotations),
            "flake8-bandit" => Ok(Plugin::Flake8Bandit),
            "flake8-blind-except" => Ok(Plugin::Flake8BlindExcept),
            "flake8-boolean-trap" => Ok(Plugin::Flake8BooleanTrap),
            "flake8-bugbear" => Ok(Plugin::Flake8Bugbear),
            "flake8-builtins" => Ok(Plugin::Flake8Builtins),
            "flake8-commas" => Ok(Plugin::Flake8Commas),
            "flake8-comprehensions" => Ok(Plugin::Flake8Comprehensions),
            "flake8-datetimez" => Ok(Plugin::Flake8Datetimez),
            "flake8-debugger" => Ok(Plugin::Flake8Debugger),
            "flake8-docstrings" => Ok(Plugin::Flake8Docstrings),
            "flake8-eradicate" => Ok(Plugin::Flake8Eradicate),
            "flake8-errmsg" => Ok(Plugin::Flake8ErrMsg),
            "flake8-executable" => Ok(Plugin::Flake8Executable),
            "flake8-implicit-str-concat" => Ok(Plugin::Flake8ImplicitStrConcat),
            "flake8-import-conventions" => Ok(Plugin::Flake8ImportConventions),
            "flake8-no-pep420" => Ok(Plugin::Flake8NoPep420),
            "flake8-pie" => Ok(Plugin::Flake8Pie),
            "flake8-print" => Ok(Plugin::Flake8Print),
            "flake8-pytest-style" => Ok(Plugin::Flake8PytestStyle),
            "flake8-quotes" => Ok(Plugin::Flake8Quotes),
            "flake8-return" => Ok(Plugin::Flake8Return),
            "flake8-simplify" => Ok(Plugin::Flake8Simplify),
            "flake8-tidy-imports" => Ok(Plugin::Flake8TidyImports),
            "flake8-type-checking" => Ok(Plugin::Flake8TypeChecking),
            "flake8-unused-arguments" => Ok(Plugin::Flake8UnusedArguments),
            "flake8-use-pathlib" => Ok(Plugin::Flake8UsePathlib),
            "mccabe" => Ok(Plugin::McCabe),
            "pep8-naming" => Ok(Plugin::PEP8Naming),
            "pandas-vet" => Ok(Plugin::PandasVet),
            "pyupgrade" => Ok(Plugin::Pyupgrade),
            "tryceratops" => Ok(Plugin::Tryceratops),
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
                Plugin::Flake82020 => "flake8-2020",
                Plugin::Flake8Annotations => "flake8-annotations",
                Plugin::Flake8Bandit => "flake8-bandit",
                Plugin::Flake8BlindExcept => "flake8-blind-except",
                Plugin::Flake8BooleanTrap => "flake8-boolean-trap",
                Plugin::Flake8Bugbear => "flake8-bugbear",
                Plugin::Flake8Builtins => "flake8-builtins",
                Plugin::Flake8Commas => "flake8-commas",
                Plugin::Flake8Comprehensions => "flake8-comprehensions",
                Plugin::Flake8Datetimez => "flake8-datetimez",
                Plugin::Flake8Debugger => "flake8-debugger",
                Plugin::Flake8Docstrings => "flake8-docstrings",
                Plugin::Flake8Eradicate => "flake8-eradicate",
                Plugin::Flake8ErrMsg => "flake8-errmsg",
                Plugin::Flake8Executable => "flake8-executable",
                Plugin::Flake8ImplicitStrConcat => "flake8-implicit-str-concat",
                Plugin::Flake8ImportConventions => "flake8-import-conventions",
                Plugin::Flake8NoPep420 => "flake8-no-pep420",
                Plugin::Flake8Pie => "flake8-pie",
                Plugin::Flake8Print => "flake8-print",
                Plugin::Flake8PytestStyle => "flake8-pytest-style",
                Plugin::Flake8Quotes => "flake8-quotes",
                Plugin::Flake8Return => "flake8-return",
                Plugin::Flake8Simplify => "flake8-simplify",
                Plugin::Flake8TidyImports => "flake8-tidy-imports",
                Plugin::Flake8TypeChecking => "flake8-type-checking",
                Plugin::Flake8UnusedArguments => "flake8-unused-arguments",
                Plugin::Flake8UsePathlib => "flake8-use-pathlib",
                Plugin::McCabe => "mccabe",
                Plugin::PEP8Naming => "pep8-naming",
                Plugin::PandasVet => "pandas-vet",
                Plugin::Pyupgrade => "pyupgrade",
                Plugin::Tryceratops => "tryceratops",
            }
        )
    }
}

// TODO(martin): Convert into `impl From<Plugin> for Linter`
impl Plugin {
    pub fn selector(&self) -> RuleSelector {
        match self {
            Plugin::Flake82020 => RuleCodePrefix::YTT.into(),
            Plugin::Flake8Annotations => RuleCodePrefix::ANN.into(),
            Plugin::Flake8Bandit => RuleCodePrefix::S.into(),
            Plugin::Flake8BlindExcept => RuleCodePrefix::BLE.into(),
            Plugin::Flake8BooleanTrap => RuleCodePrefix::FBT.into(),
            Plugin::Flake8Bugbear => RuleCodePrefix::B.into(),
            Plugin::Flake8Builtins => RuleCodePrefix::A.into(),
            Plugin::Flake8Commas => RuleCodePrefix::COM.into(),
            Plugin::Flake8Comprehensions => RuleCodePrefix::C4.into(),
            Plugin::Flake8Datetimez => RuleCodePrefix::DTZ.into(),
            Plugin::Flake8Debugger => RuleCodePrefix::T1.into(),
            Plugin::Flake8Docstrings => RuleCodePrefix::D.into(),
            Plugin::Flake8Eradicate => RuleCodePrefix::ERA.into(),
            Plugin::Flake8ErrMsg => RuleCodePrefix::EM.into(),
            Plugin::Flake8Executable => RuleCodePrefix::EXE.into(),
            Plugin::Flake8ImplicitStrConcat => RuleCodePrefix::ISC.into(),
            Plugin::Flake8ImportConventions => RuleCodePrefix::ICN.into(),
            Plugin::Flake8NoPep420 => RuleCodePrefix::INP.into(),
            Plugin::Flake8Pie => RuleCodePrefix::PIE.into(),
            Plugin::Flake8Print => RuleCodePrefix::T2.into(),
            Plugin::Flake8PytestStyle => RuleCodePrefix::PT.into(),
            Plugin::Flake8Quotes => RuleCodePrefix::Q.into(),
            Plugin::Flake8Return => RuleCodePrefix::RET.into(),
            Plugin::Flake8Simplify => RuleCodePrefix::SIM.into(),
            Plugin::Flake8TidyImports => RuleCodePrefix::TID.into(),
            Plugin::Flake8TypeChecking => RuleCodePrefix::TCH.into(),
            Plugin::Flake8UnusedArguments => RuleCodePrefix::ARG.into(),
            Plugin::Flake8UsePathlib => RuleCodePrefix::PTH.into(),
            Plugin::McCabe => RuleCodePrefix::C9.into(),
            Plugin::PEP8Naming => RuleCodePrefix::N.into(),
            Plugin::PandasVet => RuleCodePrefix::PD.into(),
            Plugin::Pyupgrade => RuleCodePrefix::UP.into(),
            Plugin::Tryceratops => RuleCodePrefix::TRY.into(),
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
pub fn infer_plugins_from_codes(selectors: &HashSet<RuleSelector>) -> Vec<Plugin> {
    // Ignore cases in which we've knowingly changed rule prefixes.
    [
        Plugin::Flake82020,
        Plugin::Flake8Annotations,
        Plugin::Flake8Bandit,
        // Plugin::Flake8BlindExcept,
        Plugin::Flake8BooleanTrap,
        Plugin::Flake8Bugbear,
        Plugin::Flake8Builtins,
        // Plugin::Flake8Commas,
        Plugin::Flake8Comprehensions,
        Plugin::Flake8Datetimez,
        Plugin::Flake8Debugger,
        Plugin::Flake8Docstrings,
        // Plugin::Flake8Eradicate,
        Plugin::Flake8ErrMsg,
        Plugin::Flake8Executable,
        Plugin::Flake8ImplicitStrConcat,
        // Plugin::Flake8ImportConventions,
        Plugin::Flake8NoPep420,
        Plugin::Flake8Pie,
        Plugin::Flake8Print,
        Plugin::Flake8PytestStyle,
        Plugin::Flake8Quotes,
        Plugin::Flake8Return,
        Plugin::Flake8Simplify,
        // Plugin::Flake8TidyImports,
        // Plugin::Flake8TypeChecking,
        Plugin::Flake8UnusedArguments,
        // Plugin::Flake8UsePathlib,
        Plugin::McCabe,
        Plugin::PEP8Naming,
        Plugin::PandasVet,
        Plugin::Tryceratops,
    ]
    .into_iter()
    .filter(|plugin| {
        for selector in selectors {
            if selector
                .into_iter()
                .any(|rule| plugin.selector().into_iter().any(|r| r == rule))
            {
                return true;
            }
        }
        false
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{infer_plugins_from_options, Plugin};

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
