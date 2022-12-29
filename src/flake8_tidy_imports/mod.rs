pub mod checks;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::checks::CheckCode;
    use crate::flake8_tidy_imports::settings::{BannedApi, Strictness};
    use crate::linter::test_path;
    use crate::{flake8_tidy_imports, Settings};

    #[test]
    fn ban_parent_imports() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![CheckCode::TID252])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::All,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![CheckCode::TID252])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn banned_api_true_positives() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID251.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "cgi".to_string(),
                            BannedApi {
                                msg: "The cgi module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "typing.TypedDict".to_string(),
                            BannedApi {
                                msg: "Use typing_extensions.TypedDict instead.".to_string(),
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                ..Settings::for_rules(vec![CheckCode::TID251])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn banned_api_false_positives() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_tidy_imports/TID251_false_positives.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_api: FxHashMap::from_iter([(
                        "typing.TypedDict".to_string(),
                        BannedApi {
                            msg: "Use typing_extensions.TypedDict instead.".to_string(),
                        },
                    )]),
                    ..Default::default()
                },
                ..Settings::for_rules(vec![CheckCode::TID251])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
