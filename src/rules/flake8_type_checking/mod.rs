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
    #[test_case(Rule::EmptyTypeCheckingBlock, Path::new("TCH005.py"); "TCH005")]
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
