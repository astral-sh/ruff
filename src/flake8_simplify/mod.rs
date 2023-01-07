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

    #[test_case(DiagnosticCode::SIM101, Path::new("SIM101.py"); "SIM101")]
    #[test_case(DiagnosticCode::SIM102, Path::new("SIM102.py"); "SIM102")]
    #[test_case(DiagnosticCode::SIM103, Path::new("SIM103.py"); "SIM103")]
    #[test_case(DiagnosticCode::SIM105, Path::new("SIM105.py"); "SIM105")]
    #[test_case(DiagnosticCode::SIM107, Path::new("SIM107.py"); "SIM107")]
    #[test_case(DiagnosticCode::SIM108, Path::new("SIM108.py"); "SIM108")]
    #[test_case(DiagnosticCode::SIM109, Path::new("SIM109.py"); "SIM109")]
    #[test_case(DiagnosticCode::SIM110, Path::new("SIM110.py"); "SIM110")]
    #[test_case(DiagnosticCode::SIM111, Path::new("SIM111.py"); "SIM111")]
    #[test_case(DiagnosticCode::SIM117, Path::new("SIM117.py"); "SIM117")]
    #[test_case(DiagnosticCode::SIM201, Path::new("SIM201.py"); "SIM201")]
    #[test_case(DiagnosticCode::SIM202, Path::new("SIM202.py"); "SIM202")]
    #[test_case(DiagnosticCode::SIM208, Path::new("SIM208.py"); "SIM208")]
    #[test_case(DiagnosticCode::SIM210, Path::new("SIM210.py"); "SIM210")]
    #[test_case(DiagnosticCode::SIM211, Path::new("SIM211.py"); "SIM211")]
    #[test_case(DiagnosticCode::SIM212, Path::new("SIM212.py"); "SIM212")]
    #[test_case(DiagnosticCode::SIM118, Path::new("SIM118.py"); "SIM118")]
    #[test_case(DiagnosticCode::SIM220, Path::new("SIM220.py"); "SIM220")]
    #[test_case(DiagnosticCode::SIM221, Path::new("SIM221.py"); "SIM221")]
    #[test_case(DiagnosticCode::SIM222, Path::new("SIM222.py"); "SIM222")]
    #[test_case(DiagnosticCode::SIM223, Path::new("SIM223.py"); "SIM223")]
    #[test_case(DiagnosticCode::SIM300, Path::new("SIM300.py"); "SIM300")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
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
