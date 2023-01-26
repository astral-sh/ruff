//! Rules from [pycodestyle](https://pypi.org/project/pycodestyle/).
pub(crate) mod rules;
pub mod settings;

pub mod helpers;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::settings::Settings;
    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::MultipleImportsOnOneLine, Path::new("E40.py"))]
    #[test_case(Rule::ModuleImportNotAtTopOfFile, Path::new("E40.py"))]
    #[test_case(Rule::ModuleImportNotAtTopOfFile, Path::new("E402.py"))]
    #[test_case(Rule::LineTooLong, Path::new("E501.py"))]
    #[test_case(Rule::NoneComparison, Path::new("E711.py"))]
    #[test_case(Rule::TrueFalseComparison, Path::new("E712.py"))]
    #[test_case(Rule::NotInTest, Path::new("E713.py"))]
    #[test_case(Rule::NotIsTest, Path::new("E714.py"))]
    #[test_case(Rule::TypeComparison, Path::new("E721.py"))]
    #[test_case(Rule::DoNotUseBareExcept, Path::new("E722.py"))]
    #[test_case(Rule::DoNotAssignLambda, Path::new("E731.py"))]
    #[test_case(Rule::AmbiguousVariableName, Path::new("E741.py"))]
    #[test_case(Rule::AmbiguousClassName, Path::new("E742.py"))]
    #[test_case(Rule::AmbiguousFunctionName, Path::new("E743.py"))]
    #[test_case(Rule::SyntaxError, Path::new("E999.py"))]
    #[test_case(Rule::NoNewLineAtEndOfFile, Path::new("W292_0.py"))]
    #[test_case(Rule::NoNewLineAtEndOfFile, Path::new("W292_1.py"))]
    #[test_case(Rule::NoNewLineAtEndOfFile, Path::new("W292_2.py"))]
    #[test_case(Rule::NoNewLineAtEndOfFile, Path::new("W292_3.py"))]
    #[test_case(Rule::NoNewLineAtEndOfFile, Path::new("W292_4.py"))]
    #[test_case(Rule::InvalidEscapeSequence, Path::new("W605_0.py"))]
    #[test_case(Rule::InvalidEscapeSequence, Path::new("W605_1.py"))]
    #[test_case(Rule::MixedSpacesAndTabs, Path::new("E101.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn constant_literals() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/constant_literals.py"),
            &settings::Settings::for_rules(vec![
                Rule::NoneComparison,
                Rule::TrueFalseComparison,
                Rule::IsLiteral,
            ]),
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test_case(false)]
    #[test_case(true)]
    fn task_tags(ignore_overlong_task_comments: bool) -> Result<()> {
        let snapshot = format!("task_tags_{ignore_overlong_task_comments}");
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/E501_1.py"),
            &settings::Settings {
                pycodestyle: Settings {
                    ignore_overlong_task_comments,
                    ..Settings::default()
                },
                ..settings::Settings::for_rule(Rule::LineTooLong)
            },
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn max_doc_length() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/pycodestyle/W505.py"),
            &settings::Settings {
                pycodestyle: Settings {
                    max_doc_length: Some(50),
                    ..Settings::default()
                },
                ..settings::Settings::for_rule(Rule::DocLineTooLong)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
