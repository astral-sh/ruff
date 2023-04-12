//! Rules from [pydocstyle](https://pypi.org/project/pydocstyle/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    use super::settings::{Convention, Settings};

    #[test_case(Rule::BlankLineAfterLastSection, Path::new("sections.py"); "D413")]
    #[test_case(Rule::NoBlankLineAfterSection, Path::new("sections.py"); "D410")]
    #[test_case(Rule::BlankLineAfterSummary, Path::new("D.py"); "D205")]
    #[test_case(Rule::NoBlankLineBeforeSection, Path::new("sections.py"); "D411")]
    #[test_case(Rule::CapitalizeSectionName, Path::new("sections.py"); "D405")]
    #[test_case(Rule::DashedUnderlineAfterSection, Path::new("sections.py"); "D407")]
    #[test_case(Rule::UndocumentedParam, Path::new("canonical_google_examples.py"); "D417_2")]
    #[test_case(Rule::UndocumentedParam, Path::new("canonical_numpy_examples.py"); "D417_1")]
    #[test_case(Rule::UndocumentedParam, Path::new("sections.py"); "D417_0")]
    #[test_case(Rule::EndsInPeriod, Path::new("D.py"); "D400_0")]
    #[test_case(Rule::EndsInPeriod, Path::new("D400.py"); "D400_1")]
    #[test_case(Rule::EndsInPunctuation, Path::new("D.py"); "D415")]
    #[test_case(Rule::FirstLineCapitalized, Path::new("D.py"); "D403_0")]
    #[test_case(Rule::FirstLineCapitalized, Path::new("D403.py"); "D403_1")]
    #[test_case(Rule::FitsOnOneLine, Path::new("D.py"); "D200")]
    #[test_case(Rule::IndentWithSpaces, Path::new("D.py"); "D206")]
    #[test_case(Rule::UndocumentedMagicMethod, Path::new("D.py"); "D105")]
    #[test_case(Rule::MultiLineSummaryFirstLine, Path::new("D.py"); "D212")]
    #[test_case(Rule::MultiLineSummarySecondLine, Path::new("D.py"); "D213")]
    #[test_case(Rule::NewLineAfterLastParagraph, Path::new("D.py"); "D209")]
    #[test_case(Rule::NewLineAfterSectionName, Path::new("sections.py"); "D406")]
    #[test_case(Rule::NoBlankLineAfterFunction, Path::new("D.py"); "D202_0")]
    #[test_case(Rule::NoBlankLineAfterFunction, Path::new("D202.py"); "D202_1")]
    #[test_case(Rule::BlankLineBeforeClass, Path::new("D.py"); "D211")]
    #[test_case(Rule::NoBlankLineBeforeFunction, Path::new("D.py"); "D201")]
    #[test_case(Rule::BlankLinesBetweenHeaderAndContent, Path::new("sections.py"); "D412")]
    #[test_case(Rule::OverIndentation, Path::new("D.py"); "D208")]
    #[test_case(Rule::NoSignature, Path::new("D.py"); "D402")]
    #[test_case(Rule::SurroundingWhitespace, Path::new("D.py"); "D210")]
    #[test_case(Rule::DocstringStartsWithThis, Path::new("D.py"); "D404")]
    #[test_case(Rule::UnderIndentation, Path::new("D.py"); "D207")]
    #[test_case(Rule::EmptyDocstring, Path::new("D.py"); "D419")]
    #[test_case(Rule::EmptyDocstringSection, Path::new("sections.py"); "D414")]
    #[test_case(Rule::NonImperativeMood, Path::new("D401.py"); "D401")]
    #[test_case(Rule::OneBlankLineAfterClass, Path::new("D.py"); "D204")]
    #[test_case(Rule::OneBlankLineBeforeClass, Path::new("D.py"); "D203")]
    #[test_case(Rule::UndocumentedPublicClass, Path::new("D.py"); "D101")]
    #[test_case(Rule::UndocumentedPublicFunction, Path::new("D.py"); "D103")]
    #[test_case(Rule::UndocumentedPublicInit, Path::new("D.py"); "D107")]
    #[test_case(Rule::UndocumentedPublicMethod, Path::new("D.py"); "D102_0")]
    #[test_case(Rule::UndocumentedPublicMethod, Path::new("setter.py"); "D102_1")]
    #[test_case(Rule::UndocumentedPublicModule, Path::new("D.py"); "D100")]
    #[test_case(Rule::UndocumentedPublicModule, Path::new("_unrelated/pkg/D100_pub.py"); "D100_ignore_unrelated_pub")]
    #[test_case(Rule::UndocumentedPublicModule, Path::new("_unrelated/pkg/_priv/no_D100_priv.py"); "no_d100_priv")]
    #[test_case(Rule::UndocumentedPublicModule, Path::new("_unrelated/_no_pkg_priv.py"); "no_d100_priv_script")]
    #[test_case(Rule::UndocumentedPublicNestedClass, Path::new("D.py"); "D106")]
    #[test_case(Rule::UndocumentedPublicPackage, Path::new("D.py"); "D104_0")]
    #[test_case(Rule::UndocumentedPublicPackage, Path::new("D104/__init__.py"); "D104_1")]
    #[test_case(Rule::SectionNameEndsInColon, Path::new("D.py"); "D416")]
    #[test_case(Rule::SectionNotOverIndented, Path::new("sections.py"); "D214")]
    #[test_case(Rule::SectionUnderlineAfterName, Path::new("sections.py"); "D408")]
    #[test_case(Rule::SectionUnderlineMatchesSectionLength, Path::new("sections.py"); "D409")]
    #[test_case(Rule::SectionUnderlineNotOverIndented, Path::new("sections.py"); "D215")]
    #[test_case(Rule::OverloadWithDocstring, Path::new("D.py"); "D418")]
    #[test_case(Rule::EscapeSequenceInDocstring, Path::new("D.py"); "D301")]
    #[test_case(Rule::TripleSingleQuotes, Path::new("D.py"); "D300")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydocstyle").join(path).as_path(),
            &settings::Settings {
                pydocstyle: Settings {
                    convention: None,
                    ignore_decorators: BTreeSet::from_iter(["functools.wraps".to_string()]),
                    property_decorators: BTreeSet::from_iter([
                        "gi.repository.GObject.Property".to_string()
                    ]),
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn bom() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/bom.py"),
            &settings::Settings::for_rule(Rule::TripleSingleQuotes),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_unspecified() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D417.py"),
            &settings::Settings {
                // When inferring the convention, we'll see a few false negatives.
                // See: https://github.com/PyCQA/pydocstyle/issues/459.
                pydocstyle: Settings {
                    convention: None,
                    ignore_decorators: BTreeSet::new(),
                    property_decorators: BTreeSet::new(),
                },
                ..settings::Settings::for_rule(Rule::UndocumentedParam)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_google() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D417.py"),
            &settings::Settings {
                // With explicit Google convention, we should flag every function.
                pydocstyle: Settings {
                    convention: Some(Convention::Google),
                    ignore_decorators: BTreeSet::new(),
                    property_decorators: BTreeSet::new(),
                },
                ..settings::Settings::for_rule(Rule::UndocumentedParam)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d417_numpy() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D417.py"),
            &settings::Settings {
                // With explicit Google convention, we shouldn't flag anything.
                pydocstyle: Settings {
                    convention: Some(Convention::Numpy),
                    ignore_decorators: BTreeSet::new(),
                    property_decorators: BTreeSet::new(),
                },
                ..settings::Settings::for_rule(Rule::UndocumentedParam)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn d209_d400() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pydocstyle/D209_D400.py"),
            &settings::Settings::for_rules([Rule::NewLineAfterLastParagraph, Rule::EndsInPeriod]),
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
