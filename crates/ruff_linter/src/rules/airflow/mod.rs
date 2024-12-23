//! Airflow-specific rules.
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::AirflowVariableNameTaskIdMismatch, Path::new("AIR001.py"))]
    #[test_case(Rule::AirflowDagNoScheduleArgument, Path::new("AIR301.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR302_args.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR302_names.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR302_class_attribute.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR302_airflow_plugin.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR302_context.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR303.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("airflow").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
