mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings::Settings;

    #[test_case(RuleCode::S101, Path::new("S101.py"); "S101")]
    #[test_case(RuleCode::S102, Path::new("S102.py"); "S102")]
    #[test_case(RuleCode::S103, Path::new("S103.py"); "S103")]
    #[test_case(RuleCode::S104, Path::new("S104.py"); "S104")]
    #[test_case(RuleCode::S105, Path::new("S105.py"); "S105")]
    #[test_case(RuleCode::S106, Path::new("S106.py"); "S106")]
    #[test_case(RuleCode::S107, Path::new("S107.py"); "S107")]
    #[test_case(RuleCode::S108, Path::new("S108.py"); "S108")]
    #[test_case(RuleCode::S113, Path::new("S113.py"); "S113")]
    #[test_case(RuleCode::S324, Path::new("S324.py"); "S324")]
    #[test_case(RuleCode::S501, Path::new("S501.py"); "S501")]
    #[test_case(RuleCode::S506, Path::new("S506.py"); "S506")]
    #[test_case(RuleCode::S508, Path::new("S508.py"); "S508")]
    #[test_case(RuleCode::S509, Path::new("S509.py"); "S509")]
    #[test_case(RuleCode::S701, Path::new("S701.py"); "S701")]
    fn rules(rule_code: RuleCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_bandit")
                .join(path)
                .as_path(),
            &Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn check_hardcoded_tmp_additional_dirs() -> Result<()> {
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_bandit/S108.py"),
            &Settings {
                flake8_bandit: super::settings::Settings {
                    hardcoded_tmp_directory: vec![
                        "/tmp".to_string(),
                        "/var/tmp".to_string(),
                        "/dev/shm".to_string(),
                        "/foo".to_string(),
                    ],
                },
                ..Settings::for_rule(RuleCode::S108)
            },
        )?;
        insta::assert_yaml_snapshot!("S108_extend", diagnostics);
        Ok(())
    }
}
