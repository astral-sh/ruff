//! Airflow-specific rules.
pub(crate) mod helpers;
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
    #[test_case(Rule::AirflowDagNoScheduleArgument, Path::new("AIR002.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_args.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_names.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_names_try.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_class_attribute.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_airflow_plugin.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_context.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302.py"))]
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
