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
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_names_fix.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_provider_names_fix.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_names_try.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_class_attribute.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_airflow_plugin.py"))]
    #[test_case(Rule::Airflow3Removal, Path::new("AIR301_context.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_amazon.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_celery.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_common_sql.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_daskexecutor.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_druid.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_fab.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_hdfs.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_hive.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_http.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_jdbc.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_kubernetes.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_mysql.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_oracle.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_papermill.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_pig.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_postgres.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_presto.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_samba.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_slack.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_smtp.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_sqlite.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_zendesk.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_standard.py"))]
    #[test_case(Rule::Airflow3MovedToProvider, Path::new("AIR302_try.py"))]
    #[test_case(Rule::Airflow3SuggestedUpdate, Path::new("AIR311_args.py"))]
    #[test_case(Rule::Airflow3SuggestedUpdate, Path::new("AIR311_names.py"))]
    #[test_case(Rule::Airflow3SuggestedUpdate, Path::new("AIR311_try.py"))]
    #[test_case(Rule::Airflow3SuggestedToMoveToProvider, Path::new("AIR312.py"))]
    #[test_case(Rule::Airflow3SuggestedToMoveToProvider, Path::new("AIR312_try.py"))]
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
