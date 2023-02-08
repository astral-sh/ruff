//! Rules from [pycodestyle](https://pypi.org/project/pycodestyle/).
pub(crate) mod rules;
pub mod settings;

pub(crate) mod helpers;
pub(crate) mod logical_lines;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use super::settings::Settings;
    use crate::registry::Rule;
    use crate::test::test_path;
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
            Path::new("pycodestyle").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[cfg(feature = "logical_lines")]
    #[test_case(Rule::IndentationWithInvalidMultiple, Path::new("E11.py"))]
    #[test_case(Rule::IndentationWithInvalidMultipleComment, Path::new("E11.py"))]
    #[test_case(Rule::MultipleLeadingHashesForBlockComment, Path::new("E26.py"))]
    #[test_case(Rule::MultipleSpacesAfterKeyword, Path::new("E27.py"))]
    #[test_case(Rule::MultipleSpacesAfterOperator, Path::new("E22.py"))]
    #[test_case(Rule::MultipleSpacesBeforeKeyword, Path::new("E27.py"))]
    #[test_case(Rule::MultipleSpacesBeforeOperator, Path::new("E22.py"))]
    #[test_case(Rule::NoIndentedBlock, Path::new("E11.py"))]
    #[test_case(Rule::NoIndentedBlockComment, Path::new("E11.py"))]
    #[test_case(Rule::NoSpaceAfterBlockComment, Path::new("E26.py"))]
    #[test_case(Rule::NoSpaceAfterInlineComment, Path::new("E26.py"))]
    #[test_case(Rule::OverIndented, Path::new("E11.py"))]
    #[test_case(Rule::TabAfterKeyword, Path::new("E27.py"))]
    #[test_case(Rule::TabAfterOperator, Path::new("E22.py"))]
    #[test_case(Rule::TabBeforeKeyword, Path::new("E27.py"))]
    #[test_case(Rule::TabBeforeOperator, Path::new("E22.py"))]
    #[test_case(Rule::TooFewSpacesBeforeInlineComment, Path::new("E26.py"))]
    #[test_case(Rule::UnexpectedIndentation, Path::new("E11.py"))]
    #[test_case(Rule::UnexpectedIndentationComment, Path::new("E11.py"))]
    #[test_case(Rule::WhitespaceAfterOpenBracket, Path::new("E20.py"))]
    #[test_case(Rule::WhitespaceBeforeCloseBracket, Path::new("E20.py"))]
    #[test_case(Rule::WhitespaceBeforePunctuation, Path::new("E20.py"))]
    fn logical(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pycodestyle").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn constant_literals() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pycodestyle/constant_literals.py"),
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
            Path::new("pycodestyle/E501_1.py"),
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
            Path::new("pycodestyle/W505.py"),
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
