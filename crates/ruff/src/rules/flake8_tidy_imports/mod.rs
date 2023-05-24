//! Rules from [flake8-tidy-imports](https://pypi.org/project/flake8-tidy-imports/).
pub mod options;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustc_hash::FxHashMap;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::rules::flake8_tidy_imports;
    use crate::rules::flake8_tidy_imports::settings::{ApiBan, Strictness};
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test]
    fn banned_api() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID251.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "cgi".to_string(),
                            ApiBan {
                                msg: "The cgi module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "typing.TypedDict".to_string(),
                            ApiBan {
                                msg: "Use typing_extensions.TypedDict instead.".to_string(),
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::BannedApi])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn banned_api_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID/my_package/sublib/api/application.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    banned_api: FxHashMap::from_iter([
                        (
                            "attrs".to_string(),
                            ApiBan {
                                msg: "The attrs module is deprecated.".to_string(),
                            },
                        ),
                        (
                            "my_package.sublib.protocol".to_string(),
                            ApiBan {
                                msg: "The protocol module is deprecated.".to_string(),
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..Settings::for_rules(vec![Rule::BannedApi])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_parent_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_all_imports() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID252.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::All,
                    ..Default::default()
                },
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn ban_parent_imports_package() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_tidy_imports/TID/my_package/sublib/api/application.py"),
            &Settings {
                flake8_tidy_imports: flake8_tidy_imports::settings::Settings {
                    ban_relative_imports: Strictness::Parents,
                    ..Default::default()
                },
                namespace_packages: vec![Path::new("my_package").to_path_buf()],
                ..Settings::for_rules(vec![Rule::RelativeImports])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
