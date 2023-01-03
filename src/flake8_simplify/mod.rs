pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::CheckCode;
    use crate::settings;

    #[test_case(CheckCode::SIM118, Path::new("SIM118.py"); "SIM118")]
    #[test_case(CheckCode::SIM222, Path::new("SIM222.py"); "SIM222")]
    #[test_case(CheckCode::SIM223, Path::new("SIM223.py"); "SIM223")]
    #[test_case(CheckCode::SIM300, Path::new("SIM300.py"); "SIM300")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_simplify")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
