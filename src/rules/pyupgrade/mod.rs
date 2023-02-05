//! Rules from [pyupgrade](https://pypi.org/project/pyupgrade/).
mod fixes;
mod helpers;
pub(crate) mod rules;
pub mod settings;
pub(crate) mod types;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::PythonVersion;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Rule::UselessMetaclassType, Path::new("UP001.py"); "UP001")]
    #[test_case(Rule::TypeOfPrimitive, Path::new("UP003.py"); "UP003")]
    #[test_case(Rule::UselessObjectInheritance, Path::new("UP004.py"); "UP004")]
    #[test_case(Rule::DeprecatedUnittestAlias, Path::new("UP005.py"); "UP005")]
    #[test_case(Rule::UsePEP585Annotation, Path::new("UP006.py"); "UP006")]
    #[test_case(Rule::UsePEP604Annotation, Path::new("UP007.py"); "UP007")]
    #[test_case(Rule::SuperCallWithParameters, Path::new("UP008.py"); "UP008")]
    #[test_case(Rule::PEP3120UnnecessaryCodingComment, Path::new("UP009_0.py"); "UP009_0")]
    #[test_case(Rule::PEP3120UnnecessaryCodingComment, Path::new("UP009_1.py"); "UP009_1")]
    #[test_case(Rule::PEP3120UnnecessaryCodingComment, Path::new("UP009_2.py"); "UP009_2")]
    #[test_case(Rule::PEP3120UnnecessaryCodingComment, Path::new("UP009_3.py"); "UP009_3")]
    #[test_case(Rule::PEP3120UnnecessaryCodingComment, Path::new("UP009_4.py"); "UP009_4")]
    #[test_case(Rule::UnnecessaryFutureImport, Path::new("UP010.py"); "UP010")]
    #[test_case(Rule::LRUCacheWithoutParameters, Path::new("UP011.py"); "UP011")]
    #[test_case(Rule::UnnecessaryEncodeUTF8, Path::new("UP012.py"); "UP012")]
    #[test_case(Rule::ConvertTypedDictFunctionalToClass, Path::new("UP013.py"); "UP013")]
    #[test_case(Rule::ConvertNamedTupleFunctionalToClass, Path::new("UP014.py"); "UP014")]
    #[test_case(Rule::RedundantOpenModes, Path::new("UP015.py"); "UP015")]
    #[test_case(Rule::NativeLiterals, Path::new("UP018.py"); "UP018")]
    #[test_case(Rule::TypingTextStrAlias, Path::new("UP019.py"); "UP019")]
    #[test_case(Rule::ReplaceUniversalNewlines, Path::new("UP021.py"); "UP021")]
    #[test_case(Rule::ReplaceStdoutStderr, Path::new("UP022.py"); "UP022")]
    #[test_case(Rule::RewriteCElementTree, Path::new("UP023.py"); "UP023")]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_0.py"); "UP024_0")]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_1.py"); "UP024_1")]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_2.py"); "UP024_2")]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_3.py"); "UP024_3")]
    #[test_case(Rule::RewriteUnicodeLiteral, Path::new("UP025.py"); "UP025")]
    #[test_case(Rule::RewriteMockImport, Path::new("UP026.py"); "UP026")]
    #[test_case(Rule::RewriteListComprehension, Path::new("UP027.py"); "UP027")]
    #[test_case(Rule::RewriteYieldFrom, Path::new("UP028_0.py"); "UP028_0")]
    #[test_case(Rule::RewriteYieldFrom, Path::new("UP028_1.py"); "UP028_1")]
    #[test_case(Rule::UnnecessaryBuiltinImport, Path::new("UP029.py"); "UP029")]
    #[test_case(Rule::FormatLiterals, Path::new("UP030_0.py"); "UP030_0")]
    #[test_case(Rule::FormatLiterals, Path::new("UP030_1.py"); "UP030_1")]
    #[test_case(Rule::FormatLiterals, Path::new("UP030_2.py"); "UP030_2")]
    #[test_case(Rule::PrintfStringFormatting, Path::new("UP031_0.py"); "UP031_0")]
    #[test_case(Rule::PrintfStringFormatting, Path::new("UP031_1.py"); "UP031_1")]
    #[test_case(Rule::FString, Path::new("UP032.py"); "UP032")]
    #[test_case(Rule::FunctoolsCache, Path::new("UP033.py"); "UP033")]
    #[test_case(Rule::ExtraneousParentheses, Path::new("UP034.py"); "UP034")]
    #[test_case(Rule::ImportReplacements, Path::new("UP035.py"); "UP035")]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_0.py"); "UP036_0")]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_1.py"); "UP036_1")]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_2.py"); "UP036_2")]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_3.py"); "UP036_3")]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_4.py"); "UP036_4")]
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037.py"); "UP037")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(Rule::UsePEP585Annotation)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py310,
                ..settings::Settings::for_rule(Rule::UsePEP585Annotation)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py37,
                ..settings::Settings::for_rule(Rule::UsePEP604Annotation)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::Settings {
                target_version: PythonVersion::Py310,
                ..settings::Settings::for_rule(Rule::UsePEP604Annotation)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }

    #[test]
    fn datetime_utc_alias_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP017.py"),
            &settings::Settings {
                target_version: PythonVersion::Py311,
                ..settings::Settings::for_rule(Rule::DatetimeTimezoneUTC)
            },
        )?;
        assert_yaml_snapshot!(diagnostics);
        Ok(())
    }
}
