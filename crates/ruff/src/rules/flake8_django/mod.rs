//! Rules from [django-flake8](https://pypi.org/project/flake8-django/)
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::ModelStringFieldNullable, Path::new("DJ01.py"); "DJ01")]
    #[test_case(Rule::ModelDunderStr, Path::new("DJ08.py"); "DJ08")]
    #[test_case(Rule::ReceiverDecoratorChecker, Path::new("DJ13.py"); "DJ13")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_django").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
