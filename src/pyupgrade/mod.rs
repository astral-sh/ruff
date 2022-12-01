mod checks;
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

    #[test_case(CheckCode::U001, Path::new("U001.py"); "U001")]
    #[test_case(CheckCode::U003, Path::new("U003.py"); "U003")]
    #[test_case(CheckCode::U004, Path::new("U004.py"); "U004")]
    #[test_case(CheckCode::U005, Path::new("U005.py"); "U005")]
    #[test_case(CheckCode::U006, Path::new("U006.py"); "U006")]
    #[test_case(CheckCode::U007, Path::new("U007.py"); "U007")]
    #[test_case(CheckCode::U008, Path::new("U008.py"); "U008")]
    #[test_case(CheckCode::U009, Path::new("U009_0.py"); "U009_0")]
    #[test_case(CheckCode::U009, Path::new("U009_1.py"); "U009_1")]
    #[test_case(CheckCode::U009, Path::new("U009_2.py"); "U009_2")]
    #[test_case(CheckCode::U009, Path::new("U009_3.py"); "U009_3")]
    #[test_case(CheckCode::U009, Path::new("U009_4.py"); "U009_4")]
    #[test_case(CheckCode::U010, Path::new("U010.py"); "U010")]
    #[test_case(CheckCode::U011, Path::new("U011_0.py"); "U011_0")]
    #[test_case(CheckCode::U011, Path::new("U011_1.py"); "U011_1")]
    #[test_case(CheckCode::U012, Path::new("U012.py"); "U012")]
    #[test_case(CheckCode::U013, Path::new("U013.py"); "U013")]
    #[test_case(CheckCode::U014, Path::new("U014.py"); "U014")]
    #[test_case(CheckCode::U015, Path::new("U015.py"); "U015")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pyupgrade")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
            true,
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
                ..settings::Settings::for_rule(CheckCode::U006)
            },
            true,
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
                ..settings::Settings::for_rule(CheckCode::U006)
            },
            true,
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
                ..settings::Settings::for_rule(CheckCode::U007)
            },
            true,
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
                ..settings::Settings::for_rule(CheckCode::U007)
            },
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
