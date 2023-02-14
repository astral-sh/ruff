//! Rules from [flake8-pyi](https://pypi.org/project/flake8-pyi/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::PrefixTypeParams, Path::new("PYI001.pyi"))]
    #[test_case(Rule::PrefixTypeParams, Path::new("PYI001.py"))]
    #[test_case(Rule::UnrecognizedPlatformCheck, Path::new("PYI007.pyi"))]
    #[test_case(Rule::UnrecognizedPlatformCheck, Path::new("PYI007.py"))]
    #[test_case(Rule::UnrecognizedPlatformName, Path::new("PYI008.pyi"))]
    #[test_case(Rule::UnrecognizedPlatformName, Path::new("PYI008.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_pyi").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
