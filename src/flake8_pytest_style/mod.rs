pub mod plugins;
pub mod settings;
pub mod types;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::flake8_pytest_style::settings::Settings;
    use crate::flake8_pytest_style::types;
    use crate::linter::test_path;
    use crate::registry::CheckCode;
    use crate::settings;

    #[test_case(CheckCode::PT001, Path::new("PT001.py"), Settings::default(), "PT001_default"; "PT001_0")]
    #[test_case(
        CheckCode::PT001,
        Path::new("PT001.py"),
        Settings {
            fixture_parentheses: false,
            ..Settings::default()
        },
        "PT001_no_parentheses";
        "PT001_1"
    )]
    #[test_case(CheckCode::PT002, Path::new("PT002.py"), Settings::default(), "PT002"; "PT002")]
    #[test_case(CheckCode::PT003, Path::new("PT003.py"), Settings::default(), "PT003"; "PT003")]
    #[test_case(CheckCode::PT004, Path::new("PT004.py"), Settings::default(), "PT004"; "PT004")]
    #[test_case(CheckCode::PT005, Path::new("PT005.py"), Settings::default(), "PT005"; "PT005")]
    #[test_case(CheckCode::PT006, Path::new("PT006.py"), Settings::default(), "PT006_default"; "PT006_0")]
    #[test_case(
        CheckCode::PT006,
        Path::new("PT006.py"),
        Settings {
            parametrize_names_type: types::ParametrizeNameType::CSV,
            ..Settings::default()
        },
        "PT006_csv";
        "PT006_1"
    )]
    #[test_case(
        CheckCode::PT006,
        Path::new("PT006.py"),
        Settings {
            parametrize_names_type: types::ParametrizeNameType::List,
            ..Settings::default()
        },
        "PT006_list";
        "PT006_2"
    )]
    #[test_case(
        CheckCode::PT007,
        Path::new("PT007.py"),
        Settings::default(),
        "PT007_list_of_tuples";
        "PT007_0"
    )]
    #[test_case(
        CheckCode::PT007,
        Path::new("PT007.py"),
        Settings {
            parametrize_values_type: types::ParametrizeValuesType::Tuple,
            ..Settings::default()
        },
        "PT007_tuple_of_tuples";
        "PT007_1"
    )]
    #[test_case(
        CheckCode::PT007,
        Path::new("PT007.py"),
        Settings {
            parametrize_values_type: types::ParametrizeValuesType::Tuple,
            parametrize_values_row_type: types::ParametrizeValuesRowType::List,
            ..Settings::default()
        },
        "PT007_tuple_of_lists";
        "PT007_2"
    )]
    #[test_case(
        CheckCode::PT007,
        Path::new("PT007.py"),
        Settings {
            parametrize_values_row_type: types::ParametrizeValuesRowType::List,
            ..Settings::default()
        },
        "PT007_list_of_lists";
        "PT007_3"
    )]
    #[test_case(
        CheckCode::PT008,
        Path::new("PT008.py"),
        Settings::default(),
        "PT008";
        "PT008"
    )]
    #[test_case(
        CheckCode::PT009,
        Path::new("PT009.py"),
        Settings::default(),
        "PT009";
        "PT009"
    )]
    #[test_case(CheckCode::PT010, Path::new("PT010.py"), Settings::default(), "PT010"; "PT0010")]
    #[test_case(
        CheckCode::PT011,
        Path::new("PT011.py"),
        Settings::default(),
        "PT011_default";
        "PT011_0"
    )]
    #[test_case(
        CheckCode::PT011,
        Path::new("PT011.py"),
        Settings {
            raises_extend_require_match_for: vec!["ZeroDivisionError".to_string()],
            ..Settings::default()
        },
        "PT011_extend_broad_exceptions";
        "PT011_1"
    )]
    #[test_case(
        CheckCode::PT011,
        Path::new("PT011.py"),
        Settings {
            raises_require_match_for: vec!["ZeroDivisionError".to_string()],
            ..Settings::default()
        },
        "PT011_replace_broad_exceptions";
        "PT011_2"
    )]
    #[test_case(
        CheckCode::PT012,
        Path::new("PT012.py"),
        Settings::default(),
        "PT012";
        "PT012"
    )]
    #[test_case(
        CheckCode::PT013,
        Path::new("PT013.py"),
        Settings::default(),
        "PT013";
        "PT013"
    )]
    #[test_case(
        CheckCode::PT015,
        Path::new("PT015.py"),
        Settings::default(),
        "PT015";
        "PT015"
    )]
    #[test_case(
        CheckCode::PT016,
        Path::new("PT016.py"),
        Settings::default(),
        "PT016";
        "PT016"
    )]
    #[test_case(
        CheckCode::PT017,
        Path::new("PT017.py"),
        Settings::default(),
        "PT017";
        "PT017"
    )]
    #[test_case(
        CheckCode::PT018,
        Path::new("PT018.py"),
        Settings::default(),
        "PT018";
        "PT018"
    )]
    #[test_case(
        CheckCode::PT019,
        Path::new("PT019.py"),
        Settings::default(),
        "PT019";
        "PT019"
    )]
    #[test_case(
        CheckCode::PT020,
        Path::new("PT020.py"),
        Settings::default(),
        "PT020";
        "PT020"
    )]
    #[test_case(
        CheckCode::PT021,
        Path::new("PT021.py"),
        Settings::default(),
        "PT021";
        "PT021"
    )]
    #[test_case(
        CheckCode::PT022,
        Path::new("PT022.py"),
        Settings::default(),
        "PT022";
        "PT022"
    )]
    #[test_case(
        CheckCode::PT023,
        Path::new("PT023.py"),
        Settings::default(),
        "PT023_default";
        "PT023_0"
    )]
    #[test_case(
        CheckCode::PT023,
        Path::new("PT023.py"),
        Settings {
            mark_parentheses: false,
            ..Settings::default()
        },
        "PT023_no_parentheses";
        "PT023_1"
    )]
    #[test_case(
        CheckCode::PT024,
        Path::new("PT024.py"),
        Settings::default(),
        "PT024";
        "PT024"
    )]
    #[test_case(
        CheckCode::PT025,
        Path::new("PT025.py"),
        Settings::default(),
        "PT025";
        "PT025"
    )]
    #[test_case(
        CheckCode::PT026,
        Path::new("PT026.py"),
        Settings::default(),
        "PT026";
        "PT026"
    )]
    fn test_pytest_style(
        check_code: CheckCode,
        path: &Path,
        plugin_settings: Settings,
        name: &str,
    ) -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_pytest_style")
                .join(path)
                .as_path(),
            &settings::Settings {
                flake8_pytest_style: plugin_settings,
                ..settings::Settings::for_rule(check_code)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(name, checks);
        Ok(())
    }
}
