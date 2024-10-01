//! Rules from [flake8-builtins](https://pypi.org/project/flake8-builtins/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::types::PythonVersion;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::BuiltinVariableShadowing, Path::new("A001.py"))]
    #[test_case(Rule::BuiltinArgumentShadowing, Path::new("A002.py"))]
    #[test_case(Rule::BuiltinAttributeShadowing, Path::new("A003.py"))]
    #[test_case(Rule::BuiltinImportShadowing, Path::new("A004.py"))]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/non_builtin/__init__.py")
    )]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/logging/__init__.py")
    )]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/string/__init__.py")
    )]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/package/bisect.py")
    )]
    #[test_case(Rule::BuiltinModuleShadowing, Path::new("A005/modules/package/xml.py"))]
    #[test_case(Rule::BuiltinLambdaArgumentShadowing, Path::new("A006.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_builtins").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::BuiltinVariableShadowing, Path::new("A001.py"))]
    #[test_case(Rule::BuiltinArgumentShadowing, Path::new("A002.py"))]
    #[test_case(Rule::BuiltinAttributeShadowing, Path::new("A003.py"))]
    #[test_case(Rule::BuiltinImportShadowing, Path::new("A004.py"))]
    #[test_case(Rule::BuiltinLambdaArgumentShadowing, Path::new("A006.py"))]
    fn builtins_ignorelist(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_{}_builtins_ignorelist",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );

        let diagnostics = test_path(
            Path::new("flake8_builtins").join(path).as_path(),
            &LinterSettings {
                flake8_builtins: super::settings::Settings {
                    builtins_ignorelist: vec!["id".to_string(), "dir".to_string()],
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![rule_code])
            },
        )?;

        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/non_builtin/__init__.py")
    )]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/logging/__init__.py")
    )]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/string/__init__.py")
    )]
    #[test_case(
        Rule::BuiltinModuleShadowing,
        Path::new("A005/modules/package/bisect.py")
    )]
    #[test_case(Rule::BuiltinModuleShadowing, Path::new("A005/modules/package/xml.py"))]
    fn builtins_allowed_modules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "{}_{}_builtins_allowed_modules",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );

        let diagnostics = test_path(
            Path::new("flake8_builtins").join(path).as_path(),
            &LinterSettings {
                flake8_builtins: super::settings::Settings {
                    builtins_allowed_modules: vec!["xml".to_string(), "logging".to_string()],
                    ..Default::default()
                },
                ..LinterSettings::for_rules(vec![rule_code])
            },
        )?;

        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::BuiltinImportShadowing, Path::new("A004.py"))]
    fn rules_py312(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}_py38", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_builtins").join(path).as_path(),
            &LinterSettings {
                target_version: PythonVersion::Py38,
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
