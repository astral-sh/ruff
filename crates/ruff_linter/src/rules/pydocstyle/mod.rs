//! Rules from [pydocstyle](https://pypi.org/project/pydocstyle/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;

    use crate::test::test_path;
    use crate::{assert_messages, settings};

    use super::settings::{Convention, Settings};

    #[test_case(Rule::BlankLineAfterLastSection, Path::new("sections.py"))]
    #[test_case(Rule::NoBlankLineAfterSection, Path::new("sections.py"))]
    #[test_case(Rule::BlankLineAfterLastSection, Path::new("D413.py"))]
    #[test_case(Rule::BlankLineAfterSummary, Path::new("D.py"))]
    #[test_case(Rule::NoBlankLineBeforeSection, Path::new("sections.py"))]
    #[test_case(Rule::CapitalizeSectionName, Path::new("sections.py"))]
    #[test_case(Rule::DashedUnderlineAfterSection, Path::new("sections.py"))]
    #[test_case(Rule::UndocumentedParam, Path::new("canonical_google_examples.py"))]
    #[test_case(Rule::UndocumentedParam, Path::new("canonical_numpy_examples.py"))]
    #[test_case(Rule::UndocumentedParam, Path::new("sections.py"))]
    #[test_case(Rule::EndsInPeriod, Path::new("D.py"))]
    #[test_case(Rule::EndsInPeriod, Path::new("D400.py"))]
    #[test_case(Rule::EndsInPeriod, Path::new("D400_415.py"))]
    #[test_case(Rule::EndsInPunctuation, Path::new("D.py"))]
    #[test_case(Rule::EndsInPunctuation, Path::new("D400_415.py"))]
    #[test_case(Rule::FirstLineCapitalized, Path::new("D.py"))]
    #[test_case(Rule::FirstLineCapitalized, Path::new("D403.py"))]
    #[test_case(Rule::FitsOnOneLine, Path::new("D.py"))]
    #[test_case(Rule::IndentWithSpaces, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedMagicMethod, Path::new("D.py"))]
    #[test_case(Rule::MultiLineSummaryFirstLine, Path::new("D.py"))]
    #[test_case(Rule::MultiLineSummarySecondLine, Path::new("D.py"))]
    #[test_case(Rule::NewLineAfterLastParagraph, Path::new("D.py"))]
    #[test_case(Rule::NewLineAfterSectionName, Path::new("sections.py"))]
    #[test_case(Rule::NoBlankLineAfterFunction, Path::new("D.py"))]
    #[test_case(Rule::FitsOnOneLine, Path::new("D200.py"))]
    #[test_case(Rule::NoBlankLineAfterFunction, Path::new("D202.py"))]
    #[test_case(Rule::BlankLineBeforeClass, Path::new("D.py"))]
    #[test_case(Rule::NoBlankLineBeforeFunction, Path::new("D.py"))]
    #[test_case(Rule::BlankLinesBetweenHeaderAndContent, Path::new("sections.py"))]
    #[test_case(Rule::BlankLinesBetweenHeaderAndContent, Path::new("sphinx.py"))]
    #[test_case(Rule::OverIndentation, Path::new("D.py"))]
    #[test_case(Rule::OverIndentation, Path::new("D208.py"))]
    #[test_case(Rule::NoSignature, Path::new("D.py"))]
    #[test_case(Rule::NoSignature, Path::new("D402.py"))]
    #[test_case(Rule::SurroundingWhitespace, Path::new("D.py"))]
    #[test_case(Rule::DocstringStartsWithThis, Path::new("D.py"))]
    #[test_case(Rule::UnderIndentation, Path::new("D.py"))]
    #[test_case(Rule::EmptyDocstring, Path::new("D.py"))]
    #[test_case(Rule::EmptyDocstringSection, Path::new("sections.py"))]
    #[test_case(Rule::NonImperativeMood, Path::new("D401.py"))]
    #[test_case(Rule::NoBlankLineAfterSection, Path::new("D410.py"))]
    #[test_case(Rule::OneBlankLineAfterClass, Path::new("D.py"))]
    #[test_case(Rule::OneBlankLineBeforeClass, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicClass, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicFunction, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicInit, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicMethod, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicMethod, Path::new("setter.py"))]
    #[test_case(Rule::UndocumentedPublicModule, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicModule, Path::new("D100.ipynb"))]
    #[test_case(
        Rule::UndocumentedPublicModule,
        Path::new("_unrelated/pkg/D100_pub.py")
    )]
    #[test_case(
        Rule::UndocumentedPublicModule,
        Path::new("_unrelated/pkg/_priv/no_D100_priv.py")
    )]
    #[test_case(
        Rule::UndocumentedPublicModule,
        Path::new("_unrelated/_no_pkg_priv.py")
    )]
    #[test_case(Rule::UndocumentedPublicNestedClass, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicPackage, Path::new("D.py"))]
    #[test_case(Rule::UndocumentedPublicPackage, Path::new("D104/__init__.py"))]
    #[test_case(Rule::SectionNameEndsInColon, Path::new("D.py"))]
    #[test_case(Rule::SectionNotOverIndented, Path::new("sections.py"))]
    #[test_case(Rule::SectionNotOverIndented, Path::new("D214_module.py"))]
    #[test_case(Rule::SectionUnderlineNotOverIndented, Path::new("D215.py"))]
    #[test_case(Rule::SectionUnderlineAfterName, Path::new("sections.py"))]
    #[test_case(Rule::SectionUnderlineMatchesSectionLength, Path::new("sections.py"))]
    #[test_case(Rule::SectionUnderlineNotOverIndented, Path::new("sections.py"))]
    #[test_case(Rule::OverloadWithDocstring, Path::new("D.py"))]
    #[test_case(Rule::EscapeSequenceInDocstring, Path::new("D.py"))]
    #[test_case(Rule::EscapeSequenceInDocstring, Path::new("D301.py"))]
    #[test_case(Rule::TripleSingleQuotes, Path::new("D.py"))]
    #[test_case(Rule::TripleSingleQuotes, Path::new("D300.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydocstyle").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: Settings::new(
                    None,
                    ["functools.wraps".to_string()],
                    ["gi.repository.GObject.Property".to_string()],
                ),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn bom() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/bom.py"),
            &settings::LinterSettings::for_rule(Rule::TripleSingleQuotes),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_unspecified() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D417.py"),
            &settings::LinterSettings {
                // When inferring the convention, we'll see a few false negatives.
                // See: https://github.com/PyCQA/pydocstyle/issues/459.
                pydocstyle: Settings::default(),
                ..settings::LinterSettings::for_rule(Rule::UndocumentedParam)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_google() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D417.py"),
            &settings::LinterSettings {
                // With explicit Google convention, we should flag every function.
                pydocstyle: Settings::new(Some(Convention::Google), [], []),
                ..settings::LinterSettings::for_rule(Rule::UndocumentedParam)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_numpy() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D417.py"),
            &settings::LinterSettings {
                // With explicit numpy convention, we shouldn't flag anything.
                pydocstyle: Settings::new(Some(Convention::Numpy), [], []),
                ..settings::LinterSettings::for_rule(Rule::UndocumentedParam)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d209_d400() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D209_D400.py"),
            &settings::LinterSettings::for_rules([
                Rule::NewLineAfterLastParagraph,
                Rule::EndsInPeriod,
            ]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn all() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/all.py"),
            &settings::LinterSettings::for_rules([
                Rule::UndocumentedPublicModule,
                Rule::UndocumentedPublicClass,
                Rule::UndocumentedPublicMethod,
                Rule::UndocumentedPublicFunction,
                Rule::UndocumentedPublicPackage,
                Rule::UndocumentedMagicMethod,
                Rule::UndocumentedPublicNestedClass,
                Rule::UndocumentedPublicInit,
            ]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
