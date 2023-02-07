//! Rules from [flake8-bandit](https://pypi.org/project/flake8-bandit/).
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_yaml_snapshot;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::AssertUsed, Path::new("S101.py"); "S101")]
    #[test_case(Rule::ExecUsed, Path::new("S102.py"); "S102")]
    #[test_case(Rule::BadFilePermissions, Path::new("S103.py"); "S103")]
    #[test_case(Rule::HardcodedBindAllInterfaces, Path::new("S104.py"); "S104")]
    #[test_case(Rule::HardcodedPasswordString, Path::new("S105.py"); "S105")]
    #[test_case(Rule::HardcodedPasswordFuncArg, Path::new("S106.py"); "S106")]
    #[test_case(Rule::HardcodedPasswordDefault, Path::new("S107.py"); "S107")]
    #[test_case(Rule::HardcodedTempFile, Path::new("S108.py"); "S108")]
    #[test_case(Rule::RequestWithoutTimeout, Path::new("S113.py"); "S113")]
    #[test_case(Rule::HashlibInsecureHashFunction, Path::new("S324.py"); "S324")]
    #[test_case(Rule::RequestWithNoCertValidation, Path::new("S501.py"); "S501")]
    #[test_case(Rule::UnsafeYAMLLoad, Path::new("S506.py"); "S506")]
    #[test_case(Rule::SnmpInsecureVersion, Path::new("S508.py"); "S508")]
    #[test_case(Rule::SnmpWeakCryptography, Path::new("S509.py"); "S509")]
    #[test_case(Rule::LoggingConfigInsecureListen, Path::new("S612.py"); "S612")]
    #[test_case(Rule::Jinja2AutoescapeFalse, Path::new("S701.py"); "S701")]
    #[test_case(Rule::TryExceptPass, Path::new("S110.py"); "S110")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bandit").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn check_hardcoded_tmp_additional_dirs() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_bandit/S108.py"),
            &Settings {
                flake8_bandit: super::settings::Settings {
                    hardcoded_tmp_directory: vec![
                        "/tmp".to_string(),
                        "/var/tmp".to_string(),
                        "/dev/shm".to_string(),
                        "/foo".to_string(),
                    ],
                    check_typed_exception: false,
                },
                ..Settings::for_rule(Rule::HardcodedTempFile)
            },
        )?;
        assert_yaml_snapshot!("S108_extend", diagnostics);
        Ok(())
    }

    #[test]
    fn check_typed_exception() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_bandit/S110.py"),
            &Settings {
                flake8_bandit: super::settings::Settings {
                    check_typed_exception: true,
                    ..Default::default()
                },
                ..Settings::for_rule(Rule::TryExceptPass)
            },
        )?;
        assert_yaml_snapshot!("S110_typed", diagnostics);
        Ok(())
    }
}
