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

    #[test_case(CheckCode::SIM102, Path::new("SIM102.py"); "SIM102")]
    #[test_case(CheckCode::SIM105, Path::new("SIM105.py"); "SIM105")]
    #[test_case(CheckCode::SIM107, Path::new("SIM107.py"); "SIM107")]
    #[test_case(CheckCode::SIM110, Path::new("SIM110.py"); "SIM110")]
    #[test_case(CheckCode::SIM111, Path::new("SIM111.py"); "SIM111")]
    #[test_case(CheckCode::SIM117, Path::new("SIM117.py"); "SIM117")]
    #[test_case(CheckCode::SIM201, Path::new("SIM201.py"); "SIM201")]
    #[test_case(CheckCode::SIM201, Path::new("SIM201_2.py"); "SIM201_2")]
    #[test_case(CheckCode::SIM202, Path::new("SIM202.py"); "SIM202")]
    #[test_case(CheckCode::SIM208, Path::new("SIM208.py"); "SIM208")]
    #[test_case(CheckCode::SIM118, Path::new("SIM118.py"); "SIM118")]
    #[test_case(CheckCode::SIM220, Path::new("SIM220.py"); "SIM220")]
    #[test_case(CheckCode::SIM221, Path::new("SIM221.py"); "SIM221")]
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
