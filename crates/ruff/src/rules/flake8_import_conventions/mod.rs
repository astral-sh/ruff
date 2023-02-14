//! Rules from [flake8-import-conventions](https://github.com/joaopalmeiro/flake8-import-conventions).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::assert_yaml_snapshot;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/defaults.py"),
            &Settings::for_rule(Rule::UnconventionalImportAlias),
        )?;
        assert_yaml_snapshot!("defaults", diagnostics);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/custom.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: None,
                    extend_aliases: Some(FxHashMap::from_iter([
                        ("dask.array".to_string(), "da".to_string()),
                        ("dask.dataframe".to_string(), "dd".to_string()),
                    ])),
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_yaml_snapshot!("custom", diagnostics);
        Ok(())
    }

    #[test]
    fn remove_defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/remove_default.py"),
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
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_yaml_snapshot!("remove_default", diagnostics);
        Ok(())
    }

    #[test]
    fn override_defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/override_default.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: None,
                    extend_aliases: Some(FxHashMap::from_iter([(
                        "numpy".to_string(),
                        "nmp".to_string(),
                    )])),
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_yaml_snapshot!("override_default", diagnostics);
        Ok(())
    }

    #[test]
    fn from_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/from_imports.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: None,
                    extend_aliases: Some(FxHashMap::from_iter([
                        ("xml.dom.minidom".to_string(), "md".to_string()),
                        (
                            "xml.dom.minidom.parseString".to_string(),
                            "pstr".to_string(),
                        ),
                    ])),
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_yaml_snapshot!("from_imports", diagnostics);
        Ok(())
    }
}
