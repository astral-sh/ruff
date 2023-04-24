//! Rules from [pep8-naming](https://pypi.org/project/pep8-naming/).
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pep8_naming;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::InvalidClassName, Path::new("N801.py"); "N801")]
    #[test_case(Rule::InvalidFunctionName, Path::new("N802.py"); "N802")]
    #[test_case(Rule::InvalidArgumentName, Path::new("N803.py"); "N803")]
    #[test_case(Rule::InvalidFirstArgumentNameForClassMethod, Path::new("N804.py"); "N804")]
    #[test_case(Rule::InvalidFirstArgumentNameForMethod, Path::new("N805.py"); "N805")]
    #[test_case(Rule::NonLowercaseVariableInFunction, Path::new("N806.py"); "N806")]
    #[test_case(Rule::DunderFunctionName, Path::new("N807.py"); "N807")]
    #[test_case(Rule::ConstantImportedAsNonConstant, Path::new("N811.py"); "N811")]
    #[test_case(Rule::LowercaseImportedAsNonLowercase, Path::new("N812.py"); "N812")]
    #[test_case(Rule::CamelcaseImportedAsLowercase, Path::new("N813.py"); "N813")]
    #[test_case(Rule::CamelcaseImportedAsConstant, Path::new("N814.py"); "N814")]
    #[test_case(Rule::MixedCaseVariableInClassScope, Path::new("N815.py"); "N815")]
    #[test_case(Rule::MixedCaseVariableInGlobalScope, Path::new("N816.py"); "N816")]
    #[test_case(Rule::CamelcaseImportedAsAcronym, Path::new("N817.py"); "N817")]
    #[test_case(Rule::ErrorSuffixOnExceptionName, Path::new("N818.py"); "N818")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/mod with spaces/__init__.py"); "N999_1")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/mod with spaces/file.py"); "N999_2")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/flake9/__init__.py"); "N999_3")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/MODULE/__init__.py"); "N999_4")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/MODULE/file.py"); "N999_5")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/mod-with-dashes/__init__.py"); "N999_6")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/valid_name/__init__.py"); "N999_7")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/no_module/test.txt"); "N999_8")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/valid_name/file-with-dashes.py"); "N999_9")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/valid_name/__main__.py"); "N999_10")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/invalid_name/0001_initial.py"); "N999_11")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/valid_name/__setup__.py"); "N999_12")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/valid_name/file-with-dashes"); "N999_13")]
    #[test_case(Rule::InvalidModuleName, Path::new("N999/module/invalid_name/import.py"); "N999_14")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pep8_naming").join(path).as_path(),
            &settings::Settings {
                ..settings::Settings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn classmethod_decorators() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pep8_naming").join("N805.py").as_path(),
            &settings::Settings {
                pep8_naming: pep8_naming::settings::Settings {
                    classmethod_decorators: vec![
                        "classmethod".to_string(),
                        "pydantic.validator".to_string(),
                    ],
                    ..Default::default()
                },
                ..settings::Settings::for_rule(Rule::InvalidFirstArgumentNameForMethod)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
