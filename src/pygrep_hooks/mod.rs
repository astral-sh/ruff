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

    #[test_case(CheckCode::PGH001, Path::new("PGH001_0.py"); "PGH001_0")]
    #[test_case(CheckCode::PGH001, Path::new("PGH001_1.py"); "PGH001_1")]
    #[test_case(CheckCode::PGH002, Path::new("PGH002_0.py"); "PGH002_0")]
    #[test_case(CheckCode::PGH002, Path::new("PGH002_1.py"); "PGH002_1")]
    #[test_case(CheckCode::PGH003, Path::new("PGH003_0.py"); "PGH003_0")]
    #[test_case(CheckCode::PGH004, Path::new("PGH004_0.py"); "PGH004_0")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/pygrep-hooks")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
