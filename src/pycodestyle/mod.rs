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

    #[test_case(CheckCode::E402, Path::new("E402.py"); "E402")]
    #[test_case(CheckCode::E501, Path::new("E501.py"); "E501")]
    #[test_case(CheckCode::E711, Path::new("E711.py"); "E711")]
    #[test_case(CheckCode::E712, Path::new("E712.py"); "E712")]
    #[test_case(CheckCode::E713, Path::new("E713.py"); "E713")]
    #[test_case(CheckCode::E714, Path::new("E714.py"); "E714")]
    #[test_case(CheckCode::E721, Path::new("E721.py"); "E721")]
    #[test_case(CheckCode::E722, Path::new("E722.py"); "E722")]
    #[test_case(CheckCode::E731, Path::new("E731.py"); "E731")]
    #[test_case(CheckCode::E741, Path::new("E741.py"); "E741")]
    #[test_case(CheckCode::E742, Path::new("E742.py"); "E742")]
    #[test_case(CheckCode::E743, Path::new("E743.py"); "E743")]
    #[test_case(CheckCode::E999, Path::new("E999.py"); "E999")]
    #[test_case(CheckCode::W292, Path::new("W292_0.py"); "W292_0")]
    #[test_case(CheckCode::W292, Path::new("W292_1.py"); "W292_1")]
    #[test_case(CheckCode::W292, Path::new("W292_2.py"); "W292_2")]
    #[test_case(CheckCode::W605, Path::new("W605_0.py"); "W605_0")]
    #[test_case(CheckCode::W605, Path::new("W605_1.py"); "W605_1")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pycodestyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
            true,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
