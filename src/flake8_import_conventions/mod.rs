pub mod checks;
pub mod settings;

#[cfg(test)]
mod tests {

    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{flake8_import_conventions, Settings};

    #[test]
    fn defaults() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/defaults.py"),
            &Settings::for_rule(CheckCode::ICN001),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("defaults", checks);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/custom.py"),
            &Settings {
                flake8_import_conventions:
                    flake8_import_conventions::settings::Settings::from_options(
                        flake8_import_conventions::settings::Options {
                            aliases: None,
                            extend_aliases: Some(FxHashMap::from_iter([
                                ("dask.array".to_string(), "da".to_string()),
                                ("dask.dataframe".to_string(), "dd".to_string()),
                            ])),
                        },
                    ),
                ..Settings::for_rule(CheckCode::ICN001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("custom", checks);
        Ok(())
    }

    #[test]
    fn remove_defaults() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/remove_default.py"),
            &Settings {
                flake8_import_conventions:
                    flake8_import_conventions::settings::Settings::from_options(
                        flake8_import_conventions::settings::Options {
                            aliases: Some(FxHashMap::from_iter([
                                ("altair".to_string(), "alt".to_string()),
                                ("matplotlib.pyplot".to_string(), "plt".to_string()),
                                ("pandas".to_string(), "pd".to_string()),
                                ("seaborn".to_string(), "sns".to_string()),
                            ])),
                            extend_aliases: None,
                        },
                    ),
                ..Settings::for_rule(CheckCode::ICN001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("remove_default", checks);
        Ok(())
    }

    #[test]
    fn override_defaults() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/override_default.py"),
            &Settings {
                flake8_import_conventions:
                    flake8_import_conventions::settings::Settings::from_options(
                        flake8_import_conventions::settings::Options {
                            aliases: None,
                            extend_aliases: Some(FxHashMap::from_iter([(
                                "numpy".to_string(),
                                "nmp".to_string(),
                            )])),
                        },
                    ),
                ..Settings::for_rule(CheckCode::ICN001)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("override_default", checks);
        Ok(())
    }
}
