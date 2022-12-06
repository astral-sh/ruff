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
            Path::new("./resources/test/fixtures/flake8_import_conventions/IC001.py"),
            &Settings::for_rule(CheckCode::IC001),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("defaults", checks);
        Ok(())
    }

    #[test]
    fn custom() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_import_conventions/IC001.py"),
            &Settings {
                flake8_import_conventions: flake8_import_conventions::settings::Settings {
                    aliases: FxHashMap::from_iter([
                        ("dask.array".to_string(), "da".to_string()),
                        ("dask.dataframe".to_string(), "dd".to_string()),
                    ]),
                },
                ..Settings::for_rule(CheckCode::IC001)
            },
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!("custom", checks);
        Ok(())
    }
}
