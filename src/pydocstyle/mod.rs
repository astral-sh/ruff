pub mod helpers;
pub mod plugins;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::pydocstyle::settings::{Convention, Settings};
    use crate::registry::DiagnosticCode;
    use crate::settings;

    #[test_case(DiagnosticCode::D100, Path::new("D.py"); "D100")]
    #[test_case(DiagnosticCode::D101, Path::new("D.py"); "D101")]
    #[test_case(DiagnosticCode::D102, Path::new("D.py"); "D102")]
    #[test_case(DiagnosticCode::D103, Path::new("D.py"); "D103")]
    #[test_case(DiagnosticCode::D104, Path::new("D.py"); "D104")]
    #[test_case(DiagnosticCode::D105, Path::new("D.py"); "D105")]
    #[test_case(DiagnosticCode::D106, Path::new("D.py"); "D106")]
    #[test_case(DiagnosticCode::D107, Path::new("D.py"); "D107")]
    #[test_case(DiagnosticCode::D201, Path::new("D.py"); "D201")]
    #[test_case(DiagnosticCode::D202, Path::new("D.py"); "D202")]
    #[test_case(DiagnosticCode::D203, Path::new("D.py"); "D203")]
    #[test_case(DiagnosticCode::D204, Path::new("D.py"); "D204")]
    #[test_case(DiagnosticCode::D205, Path::new("D.py"); "D205")]
    #[test_case(DiagnosticCode::D206, Path::new("D.py"); "D206")]
    #[test_case(DiagnosticCode::D207, Path::new("D.py"); "D207")]
    #[test_case(DiagnosticCode::D208, Path::new("D.py"); "D208")]
    #[test_case(DiagnosticCode::D209, Path::new("D.py"); "D209")]
    #[test_case(DiagnosticCode::D210, Path::new("D.py"); "D210")]
    #[test_case(DiagnosticCode::D211, Path::new("D.py"); "D211")]
    #[test_case(DiagnosticCode::D212, Path::new("D.py"); "D212")]
    #[test_case(DiagnosticCode::D213, Path::new("D.py"); "D213")]
    #[test_case(DiagnosticCode::D214, Path::new("sections.py"); "D214")]
    #[test_case(DiagnosticCode::D215, Path::new("sections.py"); "D215")]
    #[test_case(DiagnosticCode::D300, Path::new("D.py"); "D300")]
    #[test_case(DiagnosticCode::D301, Path::new("D.py"); "D301")]
    #[test_case(DiagnosticCode::D400, Path::new("D.py"); "D400_0")]
    #[test_case(DiagnosticCode::D400, Path::new("D400.py"); "D400_1")]
    #[test_case(DiagnosticCode::D402, Path::new("D.py"); "D402")]
    #[test_case(DiagnosticCode::D403, Path::new("D.py"); "D403")]
    #[test_case(DiagnosticCode::D404, Path::new("D.py"); "D404")]
    #[test_case(DiagnosticCode::D405, Path::new("sections.py"); "D405")]
    #[test_case(DiagnosticCode::D406, Path::new("sections.py"); "D406")]
    #[test_case(DiagnosticCode::D407, Path::new("sections.py"); "D407")]
    #[test_case(DiagnosticCode::D408, Path::new("sections.py"); "D408")]
    #[test_case(DiagnosticCode::D409, Path::new("sections.py"); "D409")]
    #[test_case(DiagnosticCode::D410, Path::new("sections.py"); "D410")]
    #[test_case(DiagnosticCode::D411, Path::new("sections.py"); "D411")]
    #[test_case(DiagnosticCode::D412, Path::new("sections.py"); "D412")]
    #[test_case(DiagnosticCode::D413, Path::new("sections.py"); "D413")]
    #[test_case(DiagnosticCode::D414, Path::new("sections.py"); "D414")]
    #[test_case(DiagnosticCode::D415, Path::new("D.py"); "D415")]
    #[test_case(DiagnosticCode::D416, Path::new("D.py"); "D416")]
    #[test_case(DiagnosticCode::D417, Path::new("canonical_google_examples.py"); "D417_2")]
    #[test_case(DiagnosticCode::D417, Path::new("canonical_numpy_examples.py"); "D417_1")]
    #[test_case(DiagnosticCode::D417, Path::new("sections.py"); "D417_0")]
    #[test_case(DiagnosticCode::D418, Path::new("D.py"); "D418")]
    #[test_case(DiagnosticCode::D419, Path::new("D.py"); "D419")]
    #[test_case(DiagnosticCode::D104, Path::new("D104/__init__.py"); "D104_1")]
    fn checks(check_code: DiagnosticCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let checks = test_path(
            Path::new("./resources/test/fixtures/pydocstyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(check_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn d417_unspecified() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/pydocstyle/D417.py"),
            &settings::Settings {
                // When inferring the convention, we'll see a few false negatives.
                // See: https://github.com/PyCQA/pydocstyle/issues/459.
                pydocstyle: Settings { convention: None },
                ..settings::Settings::for_rule(DiagnosticCode::D417)
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn d417_google() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/pydocstyle/D417.py"),
            &settings::Settings {
                // With explicit Google convention, we should flag every function.
                pydocstyle: Settings {
                    convention: Some(Convention::Google),
                },
                ..settings::Settings::for_rule(DiagnosticCode::D417)
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn d417_numpy() -> Result<()> {
        let checks = test_path(
            Path::new("./resources/test/fixtures/pydocstyle/D417.py"),
            &settings::Settings {
                // With explicit Google convention, we shouldn't flag anything.
                pydocstyle: Settings {
                    convention: Some(Convention::Numpy),
                },
                ..settings::Settings::for_rule(DiagnosticCode::D417)
            },
        )?;
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
