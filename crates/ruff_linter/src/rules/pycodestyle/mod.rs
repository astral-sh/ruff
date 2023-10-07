//! Rules from [pycodestyle](https://pypi.org/project/pycodestyle/).
pub(crate) mod rules;
pub mod settings;

pub(crate) mod helpers;
pub(super) mod overlong;

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::line_width::LineLength;
    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    use super::settings::Settings;

    #[test_case(Rule::AmbiguousClassName, Path::new("E742.py"))]
    #[test_case(Rule::AmbiguousFunctionName, Path::new("E743.py"))]
    #[test_case(Rule::AmbiguousVariableName, Path::new("E741.py"))]
    #[test_case(Rule::LambdaAssignment, Path::new("E731.py"))]
    #[test_case(Rule::BareExcept, Path::new("E722.py"))]
    #[test_case(Rule::BlankLineWithWhitespace, Path::new("W29.py"))]
    #[test_case(Rule::InvalidEscapeSequence, Path::new("W605_0.py"))]
    #[test_case(Rule::InvalidEscapeSequence, Path::new("W605_1.py"))]
    #[test_case(Rule::InvalidEscapeSequence, Path::new("W605_2.py"))]
    #[test_case(Rule::LineTooLong, Path::new("E501.py"))]
    #[test_case(Rule::LineTooLong, Path::new("E501_3.py"))]
    #[test_case(Rule::MixedSpacesAndTabs, Path::new("E101.py"))]
    #[test_case(Rule::ModuleImportNotAtTopOfFile, Path::new("E40.py"))]
    #[test_case(Rule::ModuleImportNotAtTopOfFile, Path::new("E402.py"))]
    #[test_case(Rule::MultipleImportsOnOneLine, Path::new("E40.py"))]
    #[test_case(Rule::MultipleStatementsOnOneLineColon, Path::new("E70.py"))]
    #[test_case(Rule::MultipleStatementsOnOneLineSemicolon, Path::new("E70.py"))]
    #[test_case(Rule::MissingNewlineAtEndOfFile, Path::new("W292_0.py"))]
    #[test_case(Rule::MissingNewlineAtEndOfFile, Path::new("W292_1.py"))]
    #[test_case(Rule::MissingNewlineAtEndOfFile, Path::new("W292_2.py"))]
    #[test_case(Rule::MissingNewlineAtEndOfFile, Path::new("W292_3.py"))]
    #[test_case(Rule::NoneComparison, Path::new("E711.py"))]
    #[test_case(Rule::NotInTest, Path::new("E713.py"))]
    #[test_case(Rule::NotIsTest, Path::new("E714.py"))]
    #[test_case(Rule::SyntaxError, Path::new("E999.py"))]
    #[test_case(Rule::TabIndentation, Path::new("W19.py"))]
    #[test_case(Rule::TrailingWhitespace, Path::new("W29.py"))]
    #[test_case(Rule::TrueFalseComparison, Path::new("E712.py"))]
    #[test_case(Rule::TypeComparison, Path::new("E721.py"))]
    #[test_case(Rule::UselessSemicolon, Path::new("E70.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pycodestyle").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn w292_4() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pycodestyle/W292_4.py"),
            &settings::LinterSettings::for_rule(Rule::MissingNewlineAtEndOfFile),
        )?;

        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::IndentationWithInvalidMultiple, Path::new("E11.py"))]
    #[test_case(Rule::IndentationWithInvalidMultipleComment, Path::new("E11.py"))]
    #[test_case(Rule::MultipleLeadingHashesForBlockComment, Path::new("E26.py"))]
    #[test_case(Rule::MultipleSpacesAfterComma, Path::new("E24.py"))]
    #[test_case(Rule::MultipleSpacesAfterKeyword, Path::new("E27.py"))]
    #[test_case(Rule::MultipleSpacesAfterOperator, Path::new("E22.py"))]
    #[test_case(Rule::MultipleSpacesBeforeKeyword, Path::new("E27.py"))]
    #[test_case(Rule::MissingWhitespaceAfterKeyword, Path::new("E27.py"))]
    #[test_case(Rule::MultipleSpacesBeforeOperator, Path::new("E22.py"))]
    #[test_case(Rule::NoIndentedBlock, Path::new("E11.py"))]
    #[test_case(Rule::NoIndentedBlockComment, Path::new("E11.py"))]
    #[test_case(Rule::NoSpaceAfterBlockComment, Path::new("E26.py"))]
    #[test_case(Rule::NoSpaceAfterInlineComment, Path::new("E26.py"))]
    #[test_case(Rule::OverIndented, Path::new("E11.py"))]
    #[test_case(Rule::TabAfterComma, Path::new("E24.py"))]
    #[test_case(Rule::TabAfterKeyword, Path::new("E27.py"))]
    #[test_case(Rule::TabAfterOperator, Path::new("E22.py"))]
    #[test_case(Rule::TabBeforeKeyword, Path::new("E27.py"))]
    #[test_case(Rule::TabBeforeOperator, Path::new("E22.py"))]
    #[test_case(Rule::MissingWhitespaceAroundOperator, Path::new("E22.py"))]
    #[test_case(Rule::MissingWhitespaceAroundArithmeticOperator, Path::new("E22.py"))]
    #[test_case(
        Rule::MissingWhitespaceAroundBitwiseOrShiftOperator,
        Path::new("E22.py")
    )]
    #[test_case(Rule::MissingWhitespaceAroundModuloOperator, Path::new("E22.py"))]
    #[test_case(Rule::MissingWhitespace, Path::new("E23.py"))]
    #[test_case(Rule::TooFewSpacesBeforeInlineComment, Path::new("E26.py"))]
    #[test_case(Rule::UnexpectedIndentation, Path::new("E11.py"))]
    #[test_case(Rule::UnexpectedIndentationComment, Path::new("E11.py"))]
    #[test_case(Rule::WhitespaceAfterOpenBracket, Path::new("E20.py"))]
    #[test_case(Rule::WhitespaceBeforeCloseBracket, Path::new("E20.py"))]
    #[test_case(Rule::WhitespaceBeforePunctuation, Path::new("E20.py"))]
    #[test_case(Rule::WhitespaceBeforeParameters, Path::new("E21.py"))]
    #[test_case(
        Rule::UnexpectedSpacesAroundKeywordParameterEquals,
        Path::new("E25.py")
    )]
    #[test_case(Rule::MissingWhitespaceAroundParameterEquals, Path::new("E25.py"))]
    fn logical(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pycodestyle").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn constant_literals() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pycodestyle/constant_literals.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::NoneComparison,
                Rule::TrueFalseComparison,
                Rule::IsLiteral,
            ]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn shebang() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pycodestyle/shebang.py"),
            &settings::LinterSettings::for_rules(vec![
                Rule::TooFewSpacesBeforeInlineComment,
                Rule::NoSpaceAfterInlineComment,
                Rule::NoSpaceAfterBlockComment,
                Rule::MultipleLeadingHashesForBlockComment,
            ]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(false)]
    #[test_case(true)]
    fn task_tags(ignore_overlong_task_comments: bool) -> Result<()> {
        let snapshot = format!("task_tags_{ignore_overlong_task_comments}");
        let diagnostics = test_path(
            Path::new("pycodestyle/E501_1.py"),
            &settings::LinterSettings {
                pycodestyle: Settings {
                    ignore_overlong_task_comments,
                    ..Settings::default()
                },
                ..settings::LinterSettings::for_rule(Rule::LineTooLong)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn max_doc_length() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pycodestyle/W505.py"),
            &settings::LinterSettings {
                pycodestyle: Settings {
                    max_doc_length: Some(LineLength::try_from(50).unwrap()),
                    ..Settings::default()
                },
                ..settings::LinterSettings::for_rule(Rule::DocLineTooLong)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn max_doc_length_with_utf_8() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pycodestyle/W505_utf_8.py"),
            &settings::LinterSettings {
                pycodestyle: Settings {
                    max_doc_length: Some(LineLength::try_from(50).unwrap()),
                    ..Settings::default()
                },
                ..settings::LinterSettings::for_rule(Rule::DocLineTooLong)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(1)]
    #[test_case(2)]
    #[test_case(4)]
    #[test_case(8)]
    fn tab_size(tab_size: u8) -> Result<()> {
        let snapshot = format!("tab_size_{tab_size}");
        let diagnostics = test_path(
            Path::new("pycodestyle/E501_2.py"),
            &settings::LinterSettings {
                tab_size: NonZeroU8::new(tab_size).unwrap().into(),
                line_length: LineLength::try_from(6).unwrap(),
                ..settings::LinterSettings::for_rule(Rule::LineTooLong)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
