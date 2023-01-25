//! Rules from [flake8-type-checking](https://pypi.org/project/flake8-type-checking/).
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::Rule;
    use crate::settings;

    #[test_case(Rule::TypingOnlyFirstPartyImport, Path::new("TYC001.py"); "TYC001")]
    #[test_case(Rule::TypingOnlyThirdPartyImport, Path::new("TYC002.py"); "TYC002")]
    #[test_case(Rule::TypingOnlyStandardLibraryImport, Path::new("TYC003.py"); "TYC003")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_1.py"); "TYC004_1")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_2.py"); "TYC004_2")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_3.py"); "TYC004_3")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_4.py"); "TYC004_4")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_5.py"); "TYC004_5")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_6.py"); "TYC004_6")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_7.py"); "TYC004_7")]
    #[test_case(Rule::RuntimeImportInTypeCheckingBlock, Path::new("TYC004_8.py"); "TYC004_8")]
    #[test_case(Rule::EmptyTypeCheckingBlock, Path::new("TYC005.py"); "TYC005")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_type_checking")
                .join(path)
                .as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
