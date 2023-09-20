//! Rules from [flake8-pytest-style](https://pypi.org/project/flake8-pytest-style/).
pub(crate) mod rules;
pub mod settings;
pub mod types;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::IdentifierPattern;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    use super::settings::Settings;
    use super::types;

    #[test_case(
        Rule::PytestFixtureIncorrectParenthesesStyle,
        Path::new("PT001.py"),
        Settings::default(),
        "PT001_default"
    )]
    #[test_case(
        Rule::PytestFixtureIncorrectParenthesesStyle,
        Path::new("PT001.py"),
        Settings {
            fixture_parentheses: false,
            ..Settings::default()
        },
        "PT001_no_parentheses"
    )]
    #[test_case(
        Rule::PytestFixturePositionalArgs,
        Path::new("PT002.py"),
        Settings::default(),
        "PT002"
    )]
    #[test_case(
        Rule::PytestExtraneousScopeFunction,
        Path::new("PT003.py"),
        Settings::default(),
        "PT003"
    )]
    #[test_case(
        Rule::PytestMissingFixtureNameUnderscore,
        Path::new("PT004.py"),
        Settings::default(),
        "PT004"
    )]
    #[test_case(
        Rule::PytestIncorrectFixtureNameUnderscore,
        Path::new("PT005.py"),
        Settings::default(),
        "PT005"
    )]
    #[test_case(
        Rule::PytestParametrizeNamesWrongType,
        Path::new("PT006.py"),
        Settings::default(),
        "PT006_default"
    )]
    #[test_case(
        Rule::PytestParametrizeNamesWrongType,
        Path::new("PT006.py"),
        Settings {
            parametrize_names_type: types::ParametrizeNameType::Csv,
            ..Settings::default()
        },
        "PT006_csv"
    )]
    #[test_case(
        Rule::PytestParametrizeNamesWrongType,
        Path::new("PT006.py"),
        Settings {
            parametrize_names_type: types::ParametrizeNameType::List,
            ..Settings::default()
        },
        "PT006_list"
    )]
    #[test_case(
        Rule::PytestParametrizeValuesWrongType,
        Path::new("PT007.py"),
        Settings::default(),
        "PT007_list_of_tuples"
    )]
    #[test_case(
        Rule::PytestParametrizeValuesWrongType,
        Path::new("PT007.py"),
        Settings {
            parametrize_values_type: types::ParametrizeValuesType::Tuple,
            ..Settings::default()
        },
        "PT007_tuple_of_tuples"
    )]
    #[test_case(
        Rule::PytestParametrizeValuesWrongType,
        Path::new("PT007.py"),
        Settings {
            parametrize_values_type: types::ParametrizeValuesType::Tuple,
            parametrize_values_row_type: types::ParametrizeValuesRowType::List,
            ..Settings::default()
        },
        "PT007_tuple_of_lists"
    )]
    #[test_case(
        Rule::PytestParametrizeValuesWrongType,
        Path::new("PT007.py"),
        Settings {
            parametrize_values_row_type: types::ParametrizeValuesRowType::List,
            ..Settings::default()
        },
        "PT007_list_of_lists"
    )]
    #[test_case(
        Rule::PytestPatchWithLambda,
        Path::new("PT008.py"),
        Settings::default(),
        "PT008"
    )]
    #[test_case(
        Rule::PytestUnittestAssertion,
        Path::new("PT009.py"),
        Settings::default(),
        "PT009";
        "PT009"
    )]
    #[test_case(
        Rule::PytestRaisesWithoutException,
        Path::new("PT010.py"),
        Settings::default(),
        "PT010"
    )]
    #[test_case(
        Rule::PytestRaisesTooBroad,
        Path::new("PT011.py"),
        Settings::default(),
        "PT011_default"
    )]
    #[test_case(
        Rule::PytestRaisesTooBroad,
        Path::new("PT011.py"),
        Settings {
            raises_extend_require_match_for: vec![IdentifierPattern::new("ZeroDivisionError").unwrap()],
            ..Settings::default()
        },
        "PT011_extend_broad_exceptions"
    )]
    #[test_case(
        Rule::PytestRaisesTooBroad,
        Path::new("PT011.py"),
        Settings {
            raises_require_match_for: vec![IdentifierPattern::new("ZeroDivisionError").unwrap()],
            ..Settings::default()
        },
        "PT011_replace_broad_exceptions"
    )]
    #[test_case(
        Rule::PytestRaisesTooBroad,
        Path::new("PT011.py"),
        Settings {
            raises_require_match_for: vec![IdentifierPattern::new("*").unwrap()],
            ..Settings::default()
        },
        "PT011_glob_all"
    )]
    #[test_case(
        Rule::PytestRaisesTooBroad,
        Path::new("PT011.py"),
        Settings {
            raises_require_match_for: vec![IdentifierPattern::new("pickle.*").unwrap()],
            ..Settings::default()
        },
        "PT011_glob_prefix"
    )]
    #[test_case(
        Rule::PytestRaisesWithMultipleStatements,
        Path::new("PT012.py"),
        Settings::default(),
        "PT012"
    )]
    #[test_case(
        Rule::PytestIncorrectPytestImport,
        Path::new("PT013.py"),
        Settings::default(),
        "PT013"
    )]
    #[test_case(
        Rule::PytestDuplicateParametrizeTestCases,
        Path::new("PT014.py"),
        Settings::default(),
        "PT014"
    )]
    #[test_case(
        Rule::PytestAssertAlwaysFalse,
        Path::new("PT015.py"),
        Settings::default(),
        "PT015"
    )]
    #[test_case(
        Rule::PytestFailWithoutMessage,
        Path::new("PT016.py"),
        Settings::default(),
        "PT016"
    )]
    #[test_case(
        Rule::PytestAssertInExcept,
        Path::new("PT017.py"),
        Settings::default(),
        "PT017"
    )]
    #[test_case(
        Rule::PytestCompositeAssertion,
        Path::new("PT018.py"),
        Settings::default(),
        "PT018"
    )]
    #[test_case(
        Rule::PytestFixtureParamWithoutValue,
        Path::new("PT019.py"),
        Settings::default(),
        "PT019"
    )]
    #[test_case(
        Rule::PytestDeprecatedYieldFixture,
        Path::new("PT020.py"),
        Settings::default(),
        "PT020"
    )]
    #[test_case(
        Rule::PytestFixtureFinalizerCallback,
        Path::new("PT021.py"),
        Settings::default(),
        "PT021"
    )]
    #[test_case(
        Rule::PytestUselessYieldFixture,
        Path::new("PT022.py"),
        Settings::default(),
        "PT022"
    )]
    #[test_case(
        Rule::PytestIncorrectMarkParenthesesStyle,
        Path::new("PT023.py"),
        Settings::default(),
        "PT023_default"
    )]
    #[test_case(
        Rule::PytestIncorrectMarkParenthesesStyle,
        Path::new("PT023.py"),
        Settings {
            mark_parentheses: false,
            ..Settings::default()
        },
        "PT023_no_parentheses"
    )]
    #[test_case(
        Rule::PytestUnnecessaryAsyncioMarkOnFixture,
        Path::new("PT024.py"),
        Settings::default(),
        "PT024"
    )]
    #[test_case(
        Rule::PytestErroneousUseFixturesOnFixture,
        Path::new("PT025.py"),
        Settings::default(),
        "PT025"
    )]
    #[test_case(
        Rule::PytestUseFixturesWithoutParameters,
        Path::new("PT026.py"),
        Settings::default(),
        "PT026"
    )]
    #[test_case(
        Rule::PytestUnittestRaisesAssertion,
        Path::new("PT027_0.py"),
        Settings::default(),
        "PT027_0"
    )]
    #[test_case(
        Rule::PytestUnittestRaisesAssertion,
        Path::new("PT027_1.py"),
        Settings::default(),
        "PT027_1"
    )]
    fn test_pytest_style(
        rule_code: Rule,
        path: &Path,
        plugin_settings: Settings,
        name: &str,
    ) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_pytest_style").join(path).as_path(),
            &settings::LinterSettings {
                flake8_pytest_style: plugin_settings,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(name, diagnostics);
        Ok(())
    }
}
