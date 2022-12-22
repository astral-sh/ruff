pub mod checks;
pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test_case(CheckCode::E401, Path::new("E40.py"))]
    #[test_case(CheckCode::E402, Path::new("E40.py"))]
    #[test_case(CheckCode::E402, Path::new("E402.py"))]
    #[test_case(CheckCode::E501, Path::new("E501.py"))]
    #[test_case(CheckCode::E711, Path::new("E711.py"))]
    #[test_case(CheckCode::E712, Path::new("E712.py"))]
    #[test_case(CheckCode::E713, Path::new("E713.py"))]
    #[test_case(CheckCode::E714, Path::new("E714.py"))]
    #[test_case(CheckCode::E721, Path::new("E721.py"))]
    #[test_case(CheckCode::E722, Path::new("E722.py"))]
    #[test_case(CheckCode::E731, Path::new("E731.py"))]
    #[test_case(CheckCode::E741, Path::new("E741.py"))]
    #[test_case(CheckCode::E742, Path::new("E742.py"))]
    #[test_case(CheckCode::E743, Path::new("E743.py"))]
    #[test_case(CheckCode::E999, Path::new("E999.py"))]
    #[test_case(CheckCode::W292, Path::new("W292_0.py"))]
    #[test_case(CheckCode::W292, Path::new("W292_1.py"))]
    #[test_case(CheckCode::W292, Path::new("W292_2.py"))]
    #[test_case(CheckCode::W605, Path::new("W605_0.py"))]
    #[test_case(CheckCode::W605, Path::new("W605_1.py"))]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pycodestyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn constant_literals() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/constant_literals.py"),
            &settings::Settings::for_rules(vec![CheckCode::E711, CheckCode::E712, CheckCode::F632]),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
