pub mod checks;
mod fixes;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test_case(CheckCode::C400, Path::new("C400.py"); "C400")]
    #[test_case(CheckCode::C401, Path::new("C401.py"); "C401")]
    #[test_case(CheckCode::C402, Path::new("C402.py"); "C402")]
    #[test_case(CheckCode::C403, Path::new("C403.py"); "C403")]
    #[test_case(CheckCode::C404, Path::new("C404.py"); "C404")]
    #[test_case(CheckCode::C405, Path::new("C405.py"); "C405")]
    #[test_case(CheckCode::C406, Path::new("C406.py"); "C406")]
    #[test_case(CheckCode::C408, Path::new("C408.py"); "C408")]
    #[test_case(CheckCode::C409, Path::new("C409.py"); "C409")]
    #[test_case(CheckCode::C410, Path::new("C410.py"); "C410")]
    #[test_case(CheckCode::C411, Path::new("C411.py"); "C411")]
    #[test_case(CheckCode::C413, Path::new("C413.py"); "C413")]
    #[test_case(CheckCode::C414, Path::new("C414.py"); "C414")]
    #[test_case(CheckCode::C415, Path::new("C415.py"); "C415")]
    #[test_case(CheckCode::C416, Path::new("C416.py"); "C416")]
    #[test_case(CheckCode::C417, Path::new("C417.py"); "C417")]

    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_comprehensions")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
