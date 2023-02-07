//! Rules from [flake8-boolean-trap](https://pypi.org/project/flake8-boolean-trap/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::BooleanPositionalArgInFunctionDefinition, Path::new("FBT.py"); "FBT001")]
    #[test_case(Rule::BooleanDefaultValueInFunctionDefinition, Path::new("FBT.py"); "FBT002")]
    #[test_case(Rule::BooleanPositionalValueInFunctionCall, Path::new("FBT.py"); "FBT003")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_boolean_trap").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
