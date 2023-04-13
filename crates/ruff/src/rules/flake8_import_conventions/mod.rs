//! Rules from [flake8-import-conventions](https://github.com/joaopalmeiro/flake8-import-conventions).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::assert_messages;
    use anyhow::Result;

    use rustc_hash::FxHashMap;

    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/defaults.py"),
            &Settings::for_rule(Rule::UnconventionalImportAlias),
        )?;
        assert_messages!("defaults", diagnostics);
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
                    banned_aliases: None,
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!("custom", diagnostics);
        Ok(())
    }

    #[test]
    fn custom_banned() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/custom_banned.py"),
            &Settings {
                flake8_import_conventions: super::settings::Options {
                    aliases: None,
                    extend_aliases: None,
                    banned_aliases: Some(FxHashMap::from_iter([
                        (
                            "typing".to_string(),
                            vec!["t".to_string(), "ty".to_string()],
                        ),
                        (
                            "numpy".to_string(),
                            vec!["nmp".to_string(), "npy".to_string()],
                        ),
                        (
                            "tensorflow.keras.backend".to_string(),
                            vec!["K".to_string()],
                        ),
                        ("torch.nn.functional".to_string(), vec!["F".to_string()]),
                    ])),
                }
                .into(),
                ..Settings::for_rule(Rule::BannedImportAlias)
            },
        )?;
        assert_messages!("custom_banned", diagnostics);
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
                    banned_aliases: None,
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!("remove_default", diagnostics);
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
                    banned_aliases: None,
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!("override_default", diagnostics);
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
                    banned_aliases: None,
                }
                .into(),
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!("from_imports", diagnostics);
        Ok(())
    }
}
