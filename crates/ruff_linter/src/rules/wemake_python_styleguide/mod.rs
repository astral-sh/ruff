//! Rules from [wemake-python-styleguide](https://pypi.org/project/wemake-python-styleguide/).
mod helpers;
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

    #[test_case(Rule::TooShortName, Path::new("WPS111.py"))]
    #[test_case(Rule::TooShortName, Path::new("WPS111/module/x/__init__.py"))]
    #[test_case(Rule::TooShortName, Path::new("WPS111/module/x/__main__.py"))]
    #[test_case(Rule::TooShortName, Path::new("WPS111/module/x/x.py"))]
    #[test_case(Rule::TooShortName, Path::new("WPS111/module/x/x.pyi"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("wemake_python_styleguide").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
