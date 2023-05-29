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
    use crate::{assert_messages, settings};

    #[test_case(Rule::TypingOnlyFirstPartyImport, Path::new("TCH001.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("TCH002.py"))]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("TCH003.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_1.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_2.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_3.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_4.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_5.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_6.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_7.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_8.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_9.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_10.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_11.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_12.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_13.py"))]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TCH004_14.pyi"))]
    #[test_case(Rule::EmptyTypeCheckingBlock, Path::new("TCH005.py"))]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("strict.py"))]
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
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("exempt_modules.py"))]
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
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test_case(
        Rule::RuntimeImportInTypeCheckingBlock,
        Path::new("runtime_evaluated_base_classes_1.py")
    )]
    #[test_case(
        Rule::TypingOnlyThirdPartyImport,
        Path::new("runtime_evaluated_base_classes_2.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("runtime_evaluated_base_classes_3.py")
    )]
    fn runtime_evaluated_base_classes(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    runtime_evaluated_base_classes: vec!["pydantic.BaseModel".to_string()],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        Rule::RuntimeImportInTypeCheckingBlock,
        Path::new("runtime_evaluated_decorators_1.py")
    )]
    #[test_case(
        Rule::TypingOnlyThirdPartyImport,
        Path::new("runtime_evaluated_decorators_2.py")
    )]
    #[test_case(
        Rule::TypingOnlyStandardLibraryImport,
        Path::new("runtime_evaluated_decorators_3.py")
    )]
    fn runtime_evaluated_decorators(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_type_checking").join(path).as_path(),
            &settings::Settings {
                flake8_type_checking: super::settings::Settings {
                    runtime_evaluated_decorators: vec![
                        "attrs.define".to_string(),
                        "attrs.frozen".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
