//! Rules from [pydoclint](https://pypi.org/project/pydoclint/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pydocstyle;
    use crate::rules::pydocstyle::settings::Convention;
    use crate::test::{test_path, test_path_as_package_init};
    use crate::{assert_diagnostics, settings};

    use super::settings::Settings;

    #[test_case(Rule::DocstringMissingException, Path::new("DOC501.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingReturns, Path::new("DOC201_google.py"), false)]
    #[test_case(Rule::DocstringExtraneousReturns, Path::new("DOC202_google.py"), false)]
    #[test_case(Rule::DocstringMissingYields, Path::new("DOC402_google.py"), false)]
    #[test_case(Rule::DocstringExtraneousYields, Path::new("DOC403_google.py"), false)]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"), false)]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"), true)]
    #[test_case(
        Rule::DocstringExtraneousException,
        Path::new("DOC502_google.py"),
        false
    )]
    fn rules_google_style(rule_code: Rule, path: &Path, in_package: bool) -> Result<()> {
        let test = match in_package {
            true => test_path_as_package_init,
            false => test_path,
        };
        let snapshot = format!(
            "{}_{}{}",
            rule_code.name(),
            path.to_string_lossy(),
            if in_package { "_package" } else { "" }
        );
        let diagnostics = test(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: pydocstyle::settings::Settings {
                    convention: Some(Convention::Google),
                    ..pydocstyle::settings::Settings::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingReturns, Path::new("DOC201_numpy.py"), false)]
    #[test_case(Rule::DocstringExtraneousReturns, Path::new("DOC202_numpy.py"), false)]
    #[test_case(Rule::DocstringMissingYields, Path::new("DOC402_numpy.py"), false)]
    #[test_case(Rule::DocstringExtraneousYields, Path::new("DOC403_numpy.py"), false)]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_numpy.py"), false)]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_numpy.py"), true)]
    #[test_case(
        Rule::DocstringExtraneousException,
        Path::new("DOC502_numpy.py"),
        false
    )]
    fn rules_numpy_style(rule_code: Rule, path: &Path, in_package: bool) -> Result<()> {
        let test = match in_package {
            true => test_path_as_package_init,
            false => test_path,
        };
        let snapshot = format!(
            "{}_{}{}",
            rule_code.name(),
            path.to_string_lossy(),
            if in_package { "_package" } else { "" }
        );
        let diagnostics = test(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: pydocstyle::settings::Settings {
                    convention: Some(Convention::Numpy),
                    ..pydocstyle::settings::Settings::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingReturns, Path::new("DOC201_google.py"), false)]
    #[test_case(Rule::DocstringMissingYields, Path::new("DOC402_google.py"), false)]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"), false)]
    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"), true)]
    fn rules_google_style_ignore_one_line(
        rule_code: Rule,
        path: &Path,
        in_package: bool,
    ) -> Result<()> {
        let test = match in_package {
            true => test_path_as_package_init,
            false => test_path,
        };
        let snapshot = format!(
            "{}_{}{}_ignore_one_line",
            rule_code.name(),
            path.to_string_lossy(),
            if in_package { "_package" } else { "" }
        );
        let diagnostics = test(
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
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
