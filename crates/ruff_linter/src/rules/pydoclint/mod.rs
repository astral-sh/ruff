//! Rules from [pydoclint](https://pypi.org/project/pydoclint/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pydocstyle::settings::{Convention, Settings};
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_google.py"))]
    #[test_case(Rule::DocstringExtraneousException, Path::new("DOC502_google.py"))]
    fn rules_google_style(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: Settings {
                    convention: Some(Convention::Google),
                    ignore_decorators: BTreeSet::new(),
                    property_decorators: BTreeSet::new(),
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::DocstringMissingException, Path::new("DOC501_numpy.py"))]
    #[test_case(Rule::DocstringExtraneousException, Path::new("DOC502_numpy.py"))]
    fn rules_numpy_style(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pydoclint").join(path).as_path(),
            &settings::LinterSettings {
                pydocstyle: Settings {
                    convention: Some(Convention::Numpy),
                    ignore_decorators: BTreeSet::new(),
                    property_decorators: BTreeSet::new(),
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
