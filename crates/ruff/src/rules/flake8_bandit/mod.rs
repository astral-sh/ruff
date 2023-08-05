//! Rules from [flake8-bandit](https://pypi.org/project/flake8-bandit/).
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::Settings;
    use crate::test::test_path;

    #[test_case(Rule::Assert, Path::new("S101.py"))]
    #[test_case(Rule::BadFilePermissions, Path::new("S103.py"))]
    #[test_case(Rule::CallWithShellEqualsTrue, Path::new("S604.py"))]
    #[test_case(Rule::ExecBuiltin, Path::new("S102.py"))]
    #[test_case(Rule::HardcodedBindAllInterfaces, Path::new("S104.py"))]
    #[test_case(Rule::HardcodedPasswordDefault, Path::new("S107.py"))]
    #[test_case(Rule::HardcodedPasswordFuncArg, Path::new("S106.py"))]
    #[test_case(Rule::HardcodedPasswordString, Path::new("S105.py"))]
    #[test_case(Rule::HardcodedSQLExpression, Path::new("S608.py"))]
    #[test_case(Rule::HardcodedTempFile, Path::new("S108.py"))]
    #[test_case(Rule::HashlibInsecureHashFunction, Path::new("S324.py"))]
    #[test_case(Rule::Jinja2AutoescapeFalse, Path::new("S701.py"))]
    #[test_case(Rule::LoggingConfigInsecureListen, Path::new("S612.py"))]
    #[test_case(Rule::ParamikoCall, Path::new("S601.py"))]
    #[test_case(Rule::RequestWithNoCertValidation, Path::new("S501.py"))]
    #[test_case(Rule::RequestWithoutTimeout, Path::new("S113.py"))]
    #[test_case(Rule::SnmpInsecureVersion, Path::new("S508.py"))]
    #[test_case(Rule::SnmpWeakCryptography, Path::new("S509.py"))]
    #[test_case(Rule::StartProcessWithAShell, Path::new("S605.py"))]
    #[test_case(Rule::StartProcessWithNoShell, Path::new("S606.py"))]
    #[test_case(Rule::StartProcessWithPartialPath, Path::new("S607.py"))]
    #[test_case(Rule::SubprocessPopenWithShellEqualsTrue, Path::new("S602.py"))]
    #[test_case(Rule::SubprocessWithoutShellEqualsTrue, Path::new("S603.py"))]
    #[test_case(Rule::SuspiciousPickleUsage, Path::new("S301.py"))]
    #[test_case(Rule::SuspiciousEvalUsage, Path::new("S307.py"))]
    #[test_case(Rule::SuspiciousTelnetUsage, Path::new("S312.py"))]
    #[test_case(Rule::TryExceptContinue, Path::new("S112.py"))]
    #[test_case(Rule::TryExceptPass, Path::new("S110.py"))]
    #[test_case(Rule::UnixCommandWildcardInjection, Path::new("S609.py"))]
    #[test_case(Rule::UnsafeYAMLLoad, Path::new("S506.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bandit").join(path).as_path(),
            &Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
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
        assert_messages!("S108_extend", diagnostics);
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
        assert_messages!("S110_typed", diagnostics);
        Ok(())
    }
}
