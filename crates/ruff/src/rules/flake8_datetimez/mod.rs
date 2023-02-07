//! Rules from [flake8-datetimez](https://pypi.org/project/flake8-datetimez/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::CallDatetimeWithoutTzinfo, Path::new("DTZ001.py"); "DTZ001")]
    #[test_case(Rule::CallDatetimeToday, Path::new("DTZ002.py"); "DTZ002")]
    #[test_case(Rule::CallDatetimeUtcnow, Path::new("DTZ003.py"); "DTZ003")]
    #[test_case(Rule::CallDatetimeUtcfromtimestamp, Path::new("DTZ004.py"); "DTZ004")]
    #[test_case(Rule::CallDatetimeNowWithoutTzinfo, Path::new("DTZ005.py"); "DTZ005")]
    #[test_case(Rule::CallDatetimeFromtimestamp, Path::new("DTZ006.py"); "DTZ006")]
    #[test_case(Rule::CallDatetimeStrptimeWithoutZone, Path::new("DTZ007.py"); "DTZ007")]
    #[test_case(Rule::CallDateToday, Path::new("DTZ011.py"); "DTZ011")]
    #[test_case(Rule::CallDateFromtimestamp, Path::new("DTZ012.py"); "DTZ012")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_datetimez").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
