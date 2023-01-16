pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::settings::{Convention, Settings};
    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;

    #[test_case(RuleCode::D100, Path::new("D.py"); "D100")]
    #[test_case(RuleCode::D101, Path::new("D.py"); "D101")]
    #[test_case(RuleCode::D102, Path::new("D.py"); "D102_0")]
    #[test_case(RuleCode::D102, Path::new("setter.py"); "D102_1")]
    #[test_case(RuleCode::D103, Path::new("D.py"); "D103")]
    #[test_case(RuleCode::D104, Path::new("D.py"); "D104")]
    #[test_case(RuleCode::D105, Path::new("D.py"); "D105")]
    #[test_case(RuleCode::D106, Path::new("D.py"); "D106")]
    #[test_case(RuleCode::D107, Path::new("D.py"); "D107")]
    #[test_case(RuleCode::D201, Path::new("D.py"); "D201")]
    #[test_case(RuleCode::D202, Path::new("D.py"); "D202")]
    #[test_case(RuleCode::D203, Path::new("D.py"); "D203")]
    #[test_case(RuleCode::D204, Path::new("D.py"); "D204")]
    #[test_case(RuleCode::D205, Path::new("D.py"); "D205")]
    #[test_case(RuleCode::D206, Path::new("D.py"); "D206")]
    #[test_case(RuleCode::D207, Path::new("D.py"); "D207")]
    #[test_case(RuleCode::D208, Path::new("D.py"); "D208")]
    #[test_case(RuleCode::D209, Path::new("D.py"); "D209")]
    #[test_case(RuleCode::D210, Path::new("D.py"); "D210")]
    #[test_case(RuleCode::D211, Path::new("D.py"); "D211")]
    #[test_case(RuleCode::D212, Path::new("D.py"); "D212")]
    #[test_case(RuleCode::D213, Path::new("D.py"); "D213")]
    #[test_case(RuleCode::D214, Path::new("sections.py"); "D214")]
    #[test_case(RuleCode::D215, Path::new("sections.py"); "D215")]
    #[test_case(RuleCode::D300, Path::new("D.py"); "D300")]
    #[test_case(RuleCode::D301, Path::new("D.py"); "D301")]
    #[test_case(RuleCode::D400, Path::new("D.py"); "D400_0")]
    #[test_case(RuleCode::D400, Path::new("D400.py"); "D400_1")]
    #[test_case(RuleCode::D402, Path::new("D.py"); "D402")]
    #[test_case(RuleCode::D403, Path::new("D.py"); "D403")]
    #[test_case(RuleCode::D404, Path::new("D.py"); "D404")]
    #[test_case(RuleCode::D405, Path::new("sections.py"); "D405")]
    #[test_case(RuleCode::D406, Path::new("sections.py"); "D406")]
    #[test_case(RuleCode::D407, Path::new("sections.py"); "D407")]
    #[test_case(RuleCode::D408, Path::new("sections.py"); "D408")]
    #[test_case(RuleCode::D409, Path::new("sections.py"); "D409")]
    #[test_case(RuleCode::D410, Path::new("sections.py"); "D410")]
    #[test_case(RuleCode::D411, Path::new("sections.py"); "D411")]
    #[test_case(RuleCode::D412, Path::new("sections.py"); "D412")]
    #[test_case(RuleCode::D413, Path::new("sections.py"); "D413")]
    #[test_case(RuleCode::D414, Path::new("sections.py"); "D414")]
    #[test_case(RuleCode::D415, Path::new("D.py"); "D415")]
    #[test_case(RuleCode::D416, Path::new("D.py"); "D416")]
    #[test_case(RuleCode::D417, Path::new("canonical_google_examples.py"); "D417_2")]
    #[test_case(RuleCode::D417, Path::new("canonical_numpy_examples.py"); "D417_1")]
    #[test_case(RuleCode::D417, Path::new("sections.py"); "D417_0")]
    #[test_case(RuleCode::D418, Path::new("D.py"); "D418")]
    #[test_case(RuleCode::D419, Path::new("D.py"); "D419")]
    #[test_case(RuleCode::D104, Path::new("D104/__init__.py"); "D104_1")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pydocstyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn d417_unspecified() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pydocstyle/D417.py"),
            &settings::Settings {
                // When inferring the convention, we'll see a few false negatives.
                // See: https://github.com/PyCQA/pydocstyle/issues/459.
                pydocstyle: Settings { convention: None },
                ..settings::Settings::for_rule(RuleCode::D417)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_google() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pydocstyle/D417.py"),
            &settings::Settings {
                // With explicit Google convention, we should flag every function.
                pydocstyle: Settings {
                    convention: Some(Convention::Google),
                },
                ..settings::Settings::for_rule(RuleCode::D417)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_numpy() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pydocstyle/D417.py"),
            &settings::Settings {
                // With explicit Google convention, we shouldn't flag anything.
                pydocstyle: Settings {
                    convention: Some(Convention::Numpy),
                },
                ..settings::Settings::for_rule(RuleCode::D417)
            },
        )?;
        insta::assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
