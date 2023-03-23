//! Rules from [flake8-newspaper-style](https://pypi.org/project/flake8-newspaper-style/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use insta::assert_yaml_snapshot;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings;
    use crate::test::test_path;

    #[test_case(Rule::FuncDefinedAbove, Path::new("NEW100.py"); "NEW100")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_newspaper_style").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
