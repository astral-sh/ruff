pub mod checks;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::settings;

    #[test_case(DiagnosticCode::ISC001, Path::new("ISC.py"); "ISC001")]
    #[test_case(DiagnosticCode::ISC002, Path::new("ISC.py"); "ISC002")]
    #[test_case(DiagnosticCode::ISC003, Path::new("ISC.py"); "ISC003")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_implicit_str_concat")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
