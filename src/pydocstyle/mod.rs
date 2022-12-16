pub mod helpers;
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

    #[test_case(CheckCode::D100, Path::new("D.py"); "D100")]
    #[test_case(CheckCode::D101, Path::new("D.py"); "D101")]
    #[test_case(CheckCode::D102, Path::new("D.py"); "D102")]
    #[test_case(CheckCode::D103, Path::new("D.py"); "D103")]
    #[test_case(CheckCode::D104, Path::new("D.py"); "D104")]
    #[test_case(CheckCode::D105, Path::new("D.py"); "D105")]
    #[test_case(CheckCode::D106, Path::new("D.py"); "D106")]
    #[test_case(CheckCode::D107, Path::new("D.py"); "D107")]
    #[test_case(CheckCode::D201, Path::new("D.py"); "D201")]
    #[test_case(CheckCode::D202, Path::new("D.py"); "D202")]
    #[test_case(CheckCode::D203, Path::new("D.py"); "D203")]
    #[test_case(CheckCode::D204, Path::new("D.py"); "D204")]
    #[test_case(CheckCode::D205, Path::new("D.py"); "D205")]
    #[test_case(CheckCode::D206, Path::new("D.py"); "D206")]
    #[test_case(CheckCode::D207, Path::new("D.py"); "D207")]
    #[test_case(CheckCode::D208, Path::new("D.py"); "D208")]
    #[test_case(CheckCode::D209, Path::new("D.py"); "D209")]
    #[test_case(CheckCode::D210, Path::new("D.py"); "D210")]
    #[test_case(CheckCode::D211, Path::new("D.py"); "D211")]
    #[test_case(CheckCode::D212, Path::new("D.py"); "D212")]
    #[test_case(CheckCode::D213, Path::new("D.py"); "D213")]
    #[test_case(CheckCode::D214, Path::new("sections.py"); "D214")]
    #[test_case(CheckCode::D215, Path::new("sections.py"); "D215")]
    #[test_case(CheckCode::D300, Path::new("D.py"); "D300")]
    #[test_case(CheckCode::D301, Path::new("D.py"); "D301")]
    #[test_case(CheckCode::D400, Path::new("D.py"); "D400_0")]
    #[test_case(CheckCode::D400, Path::new("D400.py"); "D400_1")]
    #[test_case(CheckCode::D402, Path::new("D.py"); "D402")]
    #[test_case(CheckCode::D403, Path::new("D.py"); "D403")]
    #[test_case(CheckCode::D404, Path::new("D.py"); "D404")]
    #[test_case(CheckCode::D405, Path::new("sections.py"); "D405")]
    #[test_case(CheckCode::D406, Path::new("sections.py"); "D406")]
    #[test_case(CheckCode::D407, Path::new("sections.py"); "D407")]
    #[test_case(CheckCode::D408, Path::new("sections.py"); "D408")]
    #[test_case(CheckCode::D409, Path::new("sections.py"); "D409")]
    #[test_case(CheckCode::D410, Path::new("sections.py"); "D410")]
    #[test_case(CheckCode::D411, Path::new("sections.py"); "D411")]
    #[test_case(CheckCode::D412, Path::new("sections.py"); "D412")]
    #[test_case(CheckCode::D413, Path::new("sections.py"); "D413")]
    #[test_case(CheckCode::D414, Path::new("sections.py"); "D414")]
    #[test_case(CheckCode::D415, Path::new("D.py"); "D415")]
    #[test_case(CheckCode::D416, Path::new("D.py"); "D416")]
    #[test_case(CheckCode::D417, Path::new("canonical_google_examples.py"); "D417_2")]
    #[test_case(CheckCode::D417, Path::new("canonical_numpy_examples.py"); "D417_1")]
    #[test_case(CheckCode::D417, Path::new("sections.py"); "D417_0")]
    #[test_case(CheckCode::D418, Path::new("D.py"); "D418")]
    #[test_case(CheckCode::D419, Path::new("D.py"); "D419")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/pydocstyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
