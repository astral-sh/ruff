pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::{flake8_bugbear, Settings};

    #[test_case(CheckCode::B002, Path::new("B002.py"); "B002")]
    #[test_case(CheckCode::B003, Path::new("B003.py"); "B003")]
    #[test_case(CheckCode::B004, Path::new("B004.py"); "B004")]
    #[test_case(CheckCode::B005, Path::new("B005.py"); "B005")]
    #[test_case(CheckCode::B006, Path::new("B006_B008.py"); "B006")]
    #[test_case(CheckCode::B007, Path::new("B007.py"); "B007")]
    #[test_case(CheckCode::B008, Path::new("B006_B008.py"); "B008")]
    #[test_case(CheckCode::B009, Path::new("B009_B010.py"); "B009")]
    #[test_case(CheckCode::B010, Path::new("B009_B010.py"); "B010")]
    #[test_case(CheckCode::B011, Path::new("B011.py"); "B011")]
    #[test_case(CheckCode::B012, Path::new("B012.py"); "B012")]
    #[test_case(CheckCode::B013, Path::new("B013.py"); "B013")]
    #[test_case(CheckCode::B014, Path::new("B014.py"); "B014")]
    #[test_case(CheckCode::B015, Path::new("B015.py"); "B015")]
    #[test_case(CheckCode::B016, Path::new("B016.py"); "B016")]
    #[test_case(CheckCode::B017, Path::new("B017.py"); "B017")]
    #[test_case(CheckCode::B018, Path::new("B018.py"); "B018")]
    #[test_case(CheckCode::B019, Path::new("B019.py"); "B019")]
    #[test_case(CheckCode::B020, Path::new("B020.py"); "B020")]
    #[test_case(CheckCode::B021, Path::new("B021.py"); "B021")]
    #[test_case(CheckCode::B022, Path::new("B022.py"); "B022")]
    #[test_case(CheckCode::B023, Path::new("B023.py"); "B023")]
    #[test_case(CheckCode::B024, Path::new("B024.py"); "B024")]
    #[test_case(CheckCode::B025, Path::new("B025.py"); "B025")]
    #[test_case(CheckCode::B026, Path::new("B026.py"); "B026")]
    #[test_case(CheckCode::B027, Path::new("B027.py"); "B027")]
    #[test_case(CheckCode::B904, Path::new("B904.py"); "B904")]
    #[test_case(CheckCode::B905, Path::new("B905.py"); "B905")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_bugbear")
                .join(path)
                .as_path(),
            &Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn extend_immutable_calls() -> Result<()> {
        let snapshot = "extend_immutable_calls".to_string();
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_bugbear/B008_extended.py"),
            &Settings {
                flake8_bugbear: flake8_bugbear::settings::Settings {
                    extend_immutable_calls: vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                    ],
                },
                ..Settings::for_rules(vec![CheckCode::B008])
            },
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
