//! Rules from [flake8-type-checking](https://pypi.org/project/flake8-type-checking/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::TypingOnlyFirstPartyImport, Path::new("TCH001.py"); "TCH001")]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("TCH002.py"); "TCH002")]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("TCH003.py"); "TCH003")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_1.py"); "TCH004_1")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_2.py"); "TCH004_2")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_3.py"); "TCH004_3")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_4.py"); "TCH004_4")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_5.py"); "TCH004_5")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_6.py"); "TCH004_6")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_7.py"); "TCH004_7")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_8.py"); "TCH004_8")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_9.py"); "TCH004_9")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_10.py"); "TCH004_10")]
    #[test_case(Rule::EmptyTypeCheckingBlock, Path::new("TCH005.py"); "TCH005")]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"); "strict")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"); "strict")]
    fn strict(rule_code: Rule, path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    strict: true,
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("exempt_modules.py"); "exempt_modules")]
    fn exempt_modules(rule_code: Rule, path: &Path) -> Result<()> {
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    exempt_modules: vec!["pandas".to_string()],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
