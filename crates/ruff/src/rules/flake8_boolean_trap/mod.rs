//! Rules from [flake8-boolean-trap](https://pypi.org/project/flake8-boolean-trap/).
mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::BooleanTypeHintPositionalArgument, Path::new("FBT.py"))]
    #[test_case(Rule::BooleanDefaultValuePositionalArgument, Path::new("FBT.py"))]
    #[test_case(Rule::BooleanPositionalValueInCall, Path::new("FBT.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_boolean_trap").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
