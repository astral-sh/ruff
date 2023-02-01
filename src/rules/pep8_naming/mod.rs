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
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

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
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pep8_naming").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
