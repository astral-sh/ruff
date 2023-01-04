pub mod checks;
mod helpers;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::flake8_bandit::settings::Settings;
    use crate::linter::test_path;
    use crate::registry::CheckCode;
    use crate::settings;

    #[test_case(CheckCode::S101, Path::new("S101.py"), Settings::default(), "S101"; "S101")]
    #[test_case(CheckCode::S102, Path::new("S102.py"), Settings::default(), "S102"; "S102")]
    #[test_case(CheckCode::S103, Path::new("S103.py"), Settings::default(), "S103"; "S103")]
    #[test_case(CheckCode::S104, Path::new("S104.py"), Settings::default(), "S104"; "S104")]
    #[test_case(CheckCode::S105, Path::new("S105.py"), Settings::default(), "S105"; "S105")]
    #[test_case(CheckCode::S106, Path::new("S106.py"), Settings::default(), "S106"; "S106")]
    #[test_case(CheckCode::S107, Path::new("S107.py"), Settings::default(), "S107"; "S107")]
    #[test_case(CheckCode::S108, Path::new("S108.py"), Settings::default(), "S108_default"; "S108_0")]
    #[test_case(
        CheckCode::S108, Path::new("S108.py"),
        Settings {
            hardcoded_tmp_directory: vec!["/foo".to_string()],
            ..Settings::default()
        },
        "S108_override";
        "S108_1"
    )]
    #[test_case(
        CheckCode::S108,
        Path::new("S108.py"),
        Settings {
            hardcoded_tmp_directory_extend: vec!["/foo".to_string()],
            ..Settings::default()
        },
        "S108_extend";
        "S108_2"
    )]
    fn checks(
        check_code: CheckCode,
        path: &Path,
        plugin_settings: Settings,
        label: &str,
    ) -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_bandit")
                .join(path)
                .as_path(),
            &settings::Settings {
                flake8_bandit: plugin_settings,
                ..settings::Settings::for_rule(check_code)
            },
        )?;
        insta::assert_yaml_snapshot!(label, checks);
        Ok(())
    }
}
