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
    use crate::settings::types::PreviewMode;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::Assert, Path::new("S101.py"))]
    #[test_case(Rule::BadFilePermissions, Path::new("S103.py"))]
    #[test_case(Rule::CallWithShellEqualsTrue, Path::new("S604.py"))]
    #[test_case(Rule::ExecBuiltin, Path::new("S102.py"))]
    #[test_case(Rule::FlaskDebugTrue, Path::new("S201.py"))]
    #[test_case(Rule::HardcodedBindAllInterfaces, Path::new("S104.py"))]
    #[test_case(Rule::HardcodedPasswordDefault, Path::new("S107.py"))]
    #[test_case(Rule::HardcodedPasswordFuncArg, Path::new("S106.py"))]
    #[test_case(Rule::HardcodedPasswordString, Path::new("S105.py"))]
    #[test_case(Rule::HardcodedSQLExpression, Path::new("S608.py"))]
    #[test_case(Rule::HardcodedTempFile, Path::new("S108.py"))]
    #[test_case(Rule::HashlibInsecureHashFunction, Path::new("S324.py"))]
    #[test_case(Rule::Jinja2AutoescapeFalse, Path::new("S701.py"))]
    #[test_case(Rule::MakoTemplates, Path::new("S702.py"))]
    #[test_case(Rule::LoggingConfigInsecureListen, Path::new("S612.py"))]
    #[test_case(Rule::ParamikoCall, Path::new("S601.py"))]
    #[test_case(Rule::RequestWithNoCertValidation, Path::new("S501.py"))]
    #[test_case(Rule::RequestWithoutTimeout, Path::new("S113.py"))]
    #[test_case(Rule::SSHNoHostKeyVerification, Path::new("S507.py"))]
    #[test_case(Rule::SnmpInsecureVersion, Path::new("S508.py"))]
    #[test_case(Rule::SnmpWeakCryptography, Path::new("S509.py"))]
    #[test_case(Rule::SslInsecureVersion, Path::new("S502.py"))]
    #[test_case(Rule::SslWithBadDefaults, Path::new("S503.py"))]
    #[test_case(Rule::SslWithNoVersion, Path::new("S504.py"))]
    #[test_case(Rule::StartProcessWithAShell, Path::new("S605.py"))]
    #[test_case(Rule::StartProcessWithNoShell, Path::new("S606.py"))]
    #[test_case(Rule::StartProcessWithPartialPath, Path::new("S607.py"))]
    #[test_case(Rule::SubprocessPopenWithShellEqualsTrue, Path::new("S602.py"))]
    #[test_case(Rule::SubprocessWithoutShellEqualsTrue, Path::new("S603.py"))]
    #[test_case(Rule::SuspiciousPickleUsage, Path::new("S301.py"))]
    #[test_case(Rule::SuspiciousEvalUsage, Path::new("S307.py"))]
    #[test_case(Rule::SuspiciousMarkSafeUsage, Path::new("S308.py"))]
    #[test_case(Rule::SuspiciousURLOpenUsage, Path::new("S310.py"))]
    #[test_case(Rule::SuspiciousNonCryptographicRandomUsage, Path::new("S311.py"))]
    #[test_case(Rule::SuspiciousTelnetUsage, Path::new("S312.py"))]
    #[test_case(Rule::SuspiciousTelnetlibImport, Path::new("S401.py"))]
    #[test_case(Rule::SuspiciousTelnetlibImport, Path::new("S401.pyi"))]
    #[test_case(Rule::SuspiciousFtplibImport, Path::new("S402.py"))]
    #[test_case(Rule::SuspiciousFtplibImport, Path::new("S402.pyi"))]
    #[test_case(Rule::SuspiciousPickleImport, Path::new("S403.py"))]
    #[test_case(Rule::SuspiciousPickleImport, Path::new("S403.pyi"))]
    #[test_case(Rule::SuspiciousSubprocessImport, Path::new("S404.py"))]
    #[test_case(Rule::SuspiciousSubprocessImport, Path::new("S404.pyi"))]
    #[test_case(Rule::SuspiciousXmlEtreeImport, Path::new("S405.py"))]
    #[test_case(Rule::SuspiciousXmlEtreeImport, Path::new("S405.pyi"))]
    #[test_case(Rule::SuspiciousXmlSaxImport, Path::new("S406.py"))]
    #[test_case(Rule::SuspiciousXmlSaxImport, Path::new("S406.pyi"))]
    #[test_case(Rule::SuspiciousXmlExpatImport, Path::new("S407.py"))]
    #[test_case(Rule::SuspiciousXmlExpatImport, Path::new("S407.pyi"))]
    #[test_case(Rule::SuspiciousXmlMinidomImport, Path::new("S408.py"))]
    #[test_case(Rule::SuspiciousXmlMinidomImport, Path::new("S408.pyi"))]
    #[test_case(Rule::SuspiciousXmlPulldomImport, Path::new("S409.py"))]
    #[test_case(Rule::SuspiciousXmlPulldomImport, Path::new("S409.pyi"))]
    #[test_case(Rule::SuspiciousLxmlImport, Path::new("S410.py"))]
    #[test_case(Rule::SuspiciousLxmlImport, Path::new("S410.pyi"))]
    #[test_case(Rule::SuspiciousXmlrpcImport, Path::new("S411.py"))]
    #[test_case(Rule::SuspiciousXmlrpcImport, Path::new("S411.pyi"))]
    #[test_case(Rule::SuspiciousHttpoxyImport, Path::new("S412.py"))]
    #[test_case(Rule::SuspiciousHttpoxyImport, Path::new("S412.pyi"))]
    #[test_case(Rule::SuspiciousPycryptoImport, Path::new("S413.py"))]
    #[test_case(Rule::SuspiciousPycryptoImport, Path::new("S413.pyi"))]
    #[test_case(Rule::SuspiciousPyghmiImport, Path::new("S415.py"))]
    #[test_case(Rule::SuspiciousPyghmiImport, Path::new("S415.pyi"))]
    #[test_case(Rule::TryExceptContinue, Path::new("S112.py"))]
    #[test_case(Rule::TryExceptPass, Path::new("S110.py"))]
    #[test_case(Rule::UnixCommandWildcardInjection, Path::new("S609.py"))]
    #[test_case(Rule::UnsafeYAMLLoad, Path::new("S506.py"))]
    #[test_case(Rule::WeakCryptographicKey, Path::new("S505.py"))]
    #[test_case(Rule::DjangoExtra, Path::new("S610.py"))]
    #[test_case(Rule::DjangoRawSql, Path::new("S611.py"))]
    #[test_case(Rule::TarfileUnsafeMembers, Path::new("S202.py"))]
    #[test_case(Rule::UnsafeMarkupUse, Path::new("S704.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bandit").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::SuspiciousPickleUsage, Path::new("S301.py"))]
    #[test_case(Rule::SuspiciousEvalUsage, Path::new("S307.py"))]
    #[test_case(Rule::SuspiciousMarkSafeUsage, Path::new("S308.py"))]
    #[test_case(Rule::SuspiciousURLOpenUsage, Path::new("S310.py"))]
    #[test_case(Rule::SuspiciousNonCryptographicRandomUsage, Path::new("S311.py"))]
    #[test_case(Rule::SuspiciousTelnetUsage, Path::new("S312.py"))]
    #[test_case(Rule::SubprocessWithoutShellEqualsTrue, Path::new("S603.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_bandit").join(path).as_path(),
            &LinterSettings {
                preview: PreviewMode::Enabled,
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnsafeMarkupUse, Path::new("S704_extend_markup_names.py"))]
    #[test_case(Rule::UnsafeMarkupUse, Path::new("S704_skip_early_out.py"))]
    fn extend_allowed_callable(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "extend_allow_callables__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_bandit").join(path).as_path(),
            &LinterSettings {
                flake8_bandit: super::settings::Settings {
                    extend_markup_names: vec!["webhelpers.html.literal".to_string()],
                    ..Default::default()
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::UnsafeMarkupUse, Path::new("S704_whitelisted_markup_calls.py"))]
    fn whitelisted_markup_calls(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "whitelisted_markup_calls__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_bandit").join(path).as_path(),
            &LinterSettings {
                flake8_bandit: super::settings::Settings {
                    allowed_markup_calls: vec!["bleach.clean".to_string()],
                    ..Default::default()
                },
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn check_hardcoded_tmp_additional_dirs() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_bandit/S108.py"),
            &LinterSettings {
                flake8_bandit: super::settings::Settings {
                    hardcoded_tmp_directory: vec![
                        "/tmp".to_string(),
                        "/var/tmp".to_string(),
                        "/dev/shm".to_string(),
                        "/foo".to_string(),
                    ],
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::HardcodedTempFile)
            },
        )?;
        assert_messages!("S108_extend", diagnostics);
        Ok(())
    }

    #[test]
    fn check_typed_exception() -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_bandit/S110.py"),
            &LinterSettings {
                flake8_bandit: super::settings::Settings {
                    check_typed_exception: true,
                    ..Default::default()
                },
                ..LinterSettings::for_rule(Rule::TryExceptPass)
            },
        )?;
        assert_messages!("S110_typed", diagnostics);
        Ok(())
    }
}
