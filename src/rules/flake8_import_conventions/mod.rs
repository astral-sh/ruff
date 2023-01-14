pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/defaults.py"),
            &Settings::for_rule(RuleCode::ICN001),
        )?;
        insta::assert_yaml_snapshot!("defaults", diagnostics);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/custom.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: None,
                    extend_aliases: Some(FxHashMap::from_iter([
                        ("dask.array".to_string(), "da".to_string()),
                        ("dask.dataframe".to_string(), "dd".to_string()),
                    ])),
                }
                .into(),
                ..Settings::for_rule(RuleCode::ICN001)
            },
        )?;
        insta::assert_yaml_snapshot!("custom", diagnostics);
        Ok(())
    }

    #[test]
    fn remove_defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/remove_default.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: Some(FxHashMap::from_iter([
                        ("altair".to_string(), "alt".to_string()),
                        ("matplotlib.pyplot".to_string(), "plt".to_string()),
                        ("pandas".to_string(), "pd".to_string()),
                        ("seaborn".to_string(), "sns".to_string()),
                    ])),
                    extend_aliases: None,
                }
                .into(),
                ..Settings::for_rule(RuleCode::ICN001)
            },
        )?;
        insta::assert_yaml_snapshot!("remove_default", diagnostics);
        Ok(())
    }

    #[test]
    fn override_defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/override_default.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: None,
                    extend_aliases: Some(FxHashMap::from_iter([(
                        "numpy".to_string(),
                        "nmp".to_string(),
                    )])),
                }
                .into(),
                ..Settings::for_rule(RuleCode::ICN001)
            },
        )?;
        insta::assert_yaml_snapshot!("override_default", diagnostics);
        Ok(())
    }
}
