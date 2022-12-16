mod helpers;
pub mod plugins;
mod visitor;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::Settings;

    #[test_case(CheckCode::RET501, Path::new("RET501.py"); "RET501")]
    #[test_case(CheckCode::RET502, Path::new("RET502.py"); "RET502")]
    #[test_case(CheckCode::RET503, Path::new("RET503.py"); "RET503")]
    #[test_case(CheckCode::RET504, Path::new("RET504.py"); "RET504")]
    #[test_case(CheckCode::RET505, Path::new("RET505.py"); "RET505")]
    #[test_case(CheckCode::RET506, Path::new("RET506.py"); "RET506")]
    #[test_case(CheckCode::RET507, Path::new("RET507.py"); "RET507")]
    #[test_case(CheckCode::RET508, Path::new("RET508.py"); "RET508")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/flake8_return")
                .join(path)
                .as_path(),
            &Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
