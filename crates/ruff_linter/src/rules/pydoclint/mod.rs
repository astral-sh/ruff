//! Rules from [pydoclint](https://pypi.org/project/pydoclint/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pydocstyle;
    use crate::rules::pydocstyle::settings::Convention;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    use super::settings::Settings;

    #[test_case(Rule::DocstringMissingException, Path::new("DOC501.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingReturns, Path::new("DOC201_google.py"))]
    #[test_case(Rule::DocstringExtraneousReturns, Path::new("DOC202_google.py"))]
    #[test_case(Rule::DocstringMissingYields, Path::new("DOC402_google.py"))]
    #[test_case(Rule::DocstringExtraneousYields, Path::new("DOC403_google.py"))]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"))]
    #[test_case(Rule::DocstringExtraneousException, Path::new("DOC502_google.py"))]
    fn rules_google_style(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: pydocstyle::settings::Settings {
                    convention: Some(Convention::Google),
                    ..pydocstyle::settings::Settings::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingReturns, Path::new("DOC201_numpy.py"))]
    #[test_case(Rule::DocstringExtraneousReturns, Path::new("DOC202_numpy.py"))]
    #[test_case(Rule::DocstringMissingYields, Path::new("DOC402_numpy.py"))]
    #[test_case(Rule::DocstringExtraneousYields, Path::new("DOC403_numpy.py"))]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_numpy.py"))]
    #[test_case(Rule::DocstringExtraneousException, Path::new("DOC502_numpy.py"))]
    fn rules_numpy_style(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: pydocstyle::settings::Settings {
                    convention: Some(Convention::Numpy),
                    ..pydocstyle::settings::Settings::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingReturns, Path::new("DOC201_google.py"))]
    #[test_case(Rule::DocstringMissingYields, Path::new("DOC402_google.py"))]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"))]
    fn rules_google_style_ignore_one_line(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_{}_ignore_one_line",
            rule_code.as_ref(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydoclint: Settings {
                    ignore_one_line_docstrings: true,
                },
                pydocstyle: pydocstyle::settings::Settings {
                    convention: Some(Convention::Google),
                    ..pydocstyle::settings::Settings::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
