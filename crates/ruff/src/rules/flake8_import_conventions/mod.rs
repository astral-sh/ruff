//! Rules from [flake8-import-conventions](https://github.com/joaopalmeiro/flake8-import-conventions).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::{FxHashMap, FxHashSet};

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::rules::flake8_import_conventions::settings::default_aliases;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test]
    fn defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/defaults.py"),
            &Settings::for_rule(Rule::UnconventionalImportAlias),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let mut aliases = super::settings::default_aliases();
        aliases.extend(FxHashMap::from_iter([
            ("dask.array".to_string(), "da".to_string()),
            ("dask.dataframe".to_string(), "dd".to_string()),
        ]));
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/custom.py"),
            &Settings {
                flake8_import_conventions: super::settings::Settings {
                    aliases,
                    banned_aliases: FxHashMap::default(),
                    banned_from: FxHashSet::default(),
                },
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn custom_banned() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/custom_banned.py"),
            &Settings {
                flake8_import_conventions: super::settings::Settings {
                    aliases: default_aliases(),
                    banned_aliases: FxHashMap::from_iter([
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
                    ]),
                    banned_from: FxHashSet::default(),
                },
                ..Settings::for_rule(Rule::BannedImportAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn custom_banned_from() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/custom_banned_from.py"),
            &Settings {
                flake8_import_conventions: super::settings::Settings {
                    aliases: default_aliases(),
                    banned_aliases: FxHashMap::default(),
                    banned_from: FxHashSet::from_iter([
                        "logging.config".to_string(),
                        "typing".to_string(),
                        "pandas".to_string(),
                    ]),
                },
                ..Settings::for_rule(Rule::BannedImportFrom)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn remove_defaults() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/remove_default.py"),
            &Settings {
                flake8_import_conventions: super::settings::Settings {
                    aliases: FxHashMap::from_iter([
                        ("altair".to_string(), "alt".to_string()),
                        ("matplotlib.pyplot".to_string(), "plt".to_string()),
                        ("pandas".to_string(), "pd".to_string()),
                        ("seaborn".to_string(), "sns".to_string()),
                    ]),
                    banned_aliases: FxHashMap::default(),
                    banned_from: FxHashSet::default(),
                },
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn override_defaults() -> Result<()> {
        let mut aliases = super::settings::default_aliases();
        aliases.extend(FxHashMap::from_iter([(
            "numpy".to_string(),
            "nmp".to_string(),
        )]));

        let diagnostics = test_path(
            Path::new("flake8_import_conventions/override_default.py"),
            &Settings {
                flake8_import_conventions: super::settings::Settings {
                    aliases,
                    banned_aliases: FxHashMap::default(),
                    banned_from: FxHashSet::default(),
                },
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn from_imports() -> Result<()> {
        let mut aliases = super::settings::default_aliases();
        aliases.extend(FxHashMap::from_iter([
            ("xml.dom.minidom".to_string(), "md".to_string()),
            (
                "xml.dom.minidom.parseString".to_string(),
                "pstr".to_string(),
            ),
        ]));

        let diagnostics = test_path(
            Path::new("flake8_import_conventions/from_imports.py"),
            &Settings {
                flake8_import_conventions: super::settings::Settings {
                    aliases,
                    banned_aliases: FxHashMap::default(),
                    banned_from: FxHashSet::default(),
                },
                ..Settings::for_rule(Rule::UnconventionalImportAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn tricky() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_import_conventions/tricky.py"),
            &Settings::for_rule(Rule::UnconventionalImportAlias),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
