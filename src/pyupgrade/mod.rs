pub mod checks;
pub mod fixes;
pub mod plugins;
pub mod settings;
pub mod types;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;
    use crate::settings::types::PythonVersion;

    #[test_case(CheckCode::UP001, Path::new("UP001.py"); "UP001")]
    #[test_case(CheckCode::UP003, Path::new("UP003.py"); "UP003")]
    #[test_case(CheckCode::UP004, Path::new("UP004.py"); "UP004")]
    #[test_case(CheckCode::UP005, Path::new("UP005.py"); "UP005")]
    #[test_case(CheckCode::UP006, Path::new("UP006.py"); "UP006")]
    #[test_case(CheckCode::UP007, Path::new("UP007.py"); "UP007")]
    #[test_case(CheckCode::UP008, Path::new("UP008.py"); "UP008")]
    #[test_case(CheckCode::UP009, Path::new("UP009_0.py"); "UP009_0")]
    #[test_case(CheckCode::UP009, Path::new("UP009_1.py"); "UP009_1")]
    #[test_case(CheckCode::UP009, Path::new("UP009_2.py"); "UP009_2")]
    #[test_case(CheckCode::UP009, Path::new("UP009_3.py"); "UP009_3")]
    #[test_case(CheckCode::UP009, Path::new("UP009_4.py"); "UP009_4")]
    #[test_case(CheckCode::UP010, Path::new("UP010.py"); "UP010")]
    #[test_case(CheckCode::UP011, Path::new("UP011_0.py"); "UP011_0")]
    #[test_case(CheckCode::UP011, Path::new("UP011_1.py"); "UP011_1")]
    #[test_case(CheckCode::UP012, Path::new("UP012.py"); "UP012")]
    #[test_case(CheckCode::UP013, Path::new("UP013.py"); "UP013")]
    #[test_case(CheckCode::UP014, Path::new("UP014.py"); "UP014")]
    #[test_case(CheckCode::UP015, Path::new("UP015.py"); "UP015")]
    #[test_case(CheckCode::UP016, Path::new("UP016.py"); "UP016")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyupgrade")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_p37() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(CheckCode::UP006)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_py310() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py310,
                ..settings::Settings::for_rule(CheckCode::UP006)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_p37() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(CheckCode::UP007)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_py310() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py310,
                ..settings::Settings::for_rule(CheckCode::UP007)
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
