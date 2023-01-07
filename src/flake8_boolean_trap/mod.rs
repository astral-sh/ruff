pub mod plugins;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::DiagnosticCode;
    use crate::settings;

    #[test_case(DiagnosticCode::FBT001, Path::new("FBT.py"); "FBT001")]
    #[test_case(DiagnosticCode::FBT002, Path::new("FBT.py"); "FBT002")]
    #[test_case(DiagnosticCode::FBT003, Path::new("FBT.py"); "FBT003")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_boolean_trap")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
