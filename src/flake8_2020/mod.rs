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

    #[test_case(DiagnosticCode::YTT101, Path::new("YTT101.py"); "YTT101")]
    #[test_case(DiagnosticCode::YTT102, Path::new("YTT102.py"); "YTT102")]
    #[test_case(DiagnosticCode::YTT103, Path::new("YTT103.py"); "YTT103")]
    #[test_case(DiagnosticCode::YTT201, Path::new("YTT201.py"); "YTT201")]
    #[test_case(DiagnosticCode::YTT202, Path::new("YTT202.py"); "YTT202")]
    #[test_case(DiagnosticCode::YTT203, Path::new("YTT203.py"); "YTT203")]
    #[test_case(DiagnosticCode::YTT204, Path::new("YTT204.py"); "YTT204")]
    #[test_case(DiagnosticCode::YTT301, Path::new("YTT301.py"); "YTT301")]
    #[test_case(DiagnosticCode::YTT302, Path::new("YTT302.py"); "YTT302")]
    #[test_case(DiagnosticCode::YTT303, Path::new("YTT303.py"); "YTT303")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/flake8_2020")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
