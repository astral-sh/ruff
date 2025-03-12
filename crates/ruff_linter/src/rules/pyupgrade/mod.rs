//! Rules from [pyupgrade](https://pypi.org/project/pyupgrade/).
pub(crate) mod fixes;
mod helpers;
pub(crate) mod rules;
pub mod settings;
pub(crate) mod types;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use ruff_python_ast::PythonVersion;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pyupgrade;
    use crate::settings::types::PreviewMode;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::ConvertNamedTupleFunctionalToClass, Path::new("UP014.py"))]
    #[test_case(Rule::ConvertTypedDictFunctionalToClass, Path::new("UP013.py"))]
    #[test_case(Rule::DeprecatedCElementTree, Path::new("UP023.py"))]
    #[test_case(Rule::DeprecatedImport, Path::new("UP035.py"))]
    #[test_case(Rule::DeprecatedMockImport, Path::new("UP026.py"))]
    #[test_case(Rule::DeprecatedUnittestAlias, Path::new("UP005.py"))]
    #[test_case(Rule::ExtraneousParentheses, Path::new("UP034.py"))]
    #[test_case(Rule::FString, Path::new("UP032_0.py"))]
    #[test_case(Rule::FString, Path::new("UP032_1.py"))]
    #[test_case(Rule::FString, Path::new("UP032_2.py"))]
    #[test_case(Rule::FString, Path::new("UP032_3.py"))]
    #[test_case(Rule::FormatLiterals, Path::new("UP030_0.py"))]
    #[test_case(Rule::FormatLiterals, Path::new("UP030_1.py"))]
    #[test_case(Rule::LRUCacheWithMaxsizeNone, Path::new("UP033_0.py"))]
    #[test_case(Rule::LRUCacheWithMaxsizeNone, Path::new("UP033_1.py"))]
    #[test_case(Rule::LRUCacheWithoutParameters, Path::new("UP011.py"))]
    #[test_case(Rule::NativeLiterals, Path::new("UP018.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_0.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_1.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_2.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_3.py"))]
    #[test_case(Rule::NonPEP604AnnotationUnion, Path::new("UP007.py"))]
    #[test_case(Rule::NonPEP604AnnotationUnion, Path::new("UP045.py"))]
    #[test_case(Rule::NonPEP604Isinstance, Path::new("UP038.py"))]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_0.py"))]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_1.py"))]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_2.py"))]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_3.py"))]
    #[test_case(Rule::OSErrorAlias, Path::new("UP024_4.py"))]
    #[test_case(Rule::OpenAlias, Path::new("UP020.py"))]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_0.py"))]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_1.py"))]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_2.py"))]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_3.py"))]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_4.py"))]
    #[test_case(Rule::OutdatedVersionBlock, Path::new("UP036_5.py"))]
    #[test_case(Rule::PrintfStringFormatting, Path::new("UP031_0.py"))]
    #[test_case(Rule::PrintfStringFormatting, Path::new("UP031_1.py"))]
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_0.py"))]
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_1.py"))]
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_2.pyi"))]
    #[test_case(Rule::RedundantOpenModes, Path::new("UP015.py"))]
    #[test_case(Rule::RedundantOpenModes, Path::new("UP015_1.py"))]
    #[test_case(Rule::ReplaceStdoutStderr, Path::new("UP022.py"))]
    #[test_case(Rule::ReplaceUniversalNewlines, Path::new("UP021.py"))]
    #[test_case(Rule::SuperCallWithParameters, Path::new("UP008.py"))]
    #[test_case(Rule::TimeoutErrorAlias, Path::new("UP041.py"))]
    #[test_case(Rule::ReplaceStrEnum, Path::new("UP042.py"))]
    #[test_case(Rule::TypeOfPrimitive, Path::new("UP003.py"))]
    #[test_case(Rule::TypingTextStrAlias, Path::new("UP019.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_0.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_1.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_2.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_3.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_4.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_5.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_6.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_7.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_8.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_9.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_10.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_other_other.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_other_utf8.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_utf8_other.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_utf8_utf8.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_utf8_utf8_other.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_utf8_code_other.py"))]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_code_utf8_utf8.py"))]
    #[test_case(
        Rule::UTF8EncodingDeclaration,
        Path::new("UP009_hashbang_utf8_other.py")
    )]
    #[test_case(Rule::UTF8EncodingDeclaration, Path::new("UP009_many_empty_lines.py"))]
    #[test_case(Rule::UnicodeKindPrefix, Path::new("UP025.py"))]
    #[test_case(Rule::UnnecessaryBuiltinImport, Path::new("UP029.py"))]
    #[test_case(Rule::UnnecessaryClassParentheses, Path::new("UP039.py"))]
    #[test_case(Rule::UnnecessaryDefaultTypeArgs, Path::new("UP043.py"))]
    #[test_case(Rule::UnnecessaryEncodeUTF8, Path::new("UP012.py"))]
    #[test_case(Rule::UnnecessaryFutureImport, Path::new("UP010.py"))]
    #[test_case(Rule::UselessMetaclassType, Path::new("UP001.py"))]
    #[test_case(Rule::UselessObjectInheritance, Path::new("UP004.py"))]
    #[test_case(Rule::YieldInForLoop, Path::new("UP028_0.py"))]
    #[test_case(Rule::YieldInForLoop, Path::new("UP028_1.py"))]
    #[test_case(Rule::NonPEP695TypeAlias, Path::new("UP040.py"))]
    #[test_case(Rule::NonPEP695TypeAlias, Path::new("UP040.pyi"))]
    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_0.py"))]
    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_1.py"))]
    #[test_case(Rule::NonPEP695GenericFunction, Path::new("UP047.py"))]
    #[test_case(Rule::PrivateTypeParameter, Path::new("UP049_0.py"))]
    #[test_case(Rule::PrivateTypeParameter, Path::new("UP049_1.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().to_string();
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn up007_preview() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP045.py"),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(Rule::NonPEP604AnnotationUnion)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn async_timeout_error_alias_not_applied_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP041.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY310,
                ..settings::LinterSettings::for_rule(Rule::TimeoutErrorAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn non_pep695_type_alias_not_applied_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP040.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY311,
                ..settings::LinterSettings::for_rule(Rule::NonPEP695TypeAlias)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_keep_runtime_typing_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                pyupgrade: pyupgrade::settings::Settings {
                    keep_runtime_typing: true,
                },
                unresolved_target_version: PythonVersion::PY37,
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_keep_runtime_typing_p310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                pyupgrade: pyupgrade::settings::Settings {
                    keep_runtime_typing: true,
                },
                unresolved_target_version: PythonVersion::PY310,
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY37,
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY310,
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY37,
                ..settings::LinterSettings::for_rules([
                    Rule::NonPEP604AnnotationUnion,
                    Rule::NonPEP604AnnotationOptional,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY310,
                ..settings::LinterSettings::for_rules([
                    Rule::NonPEP604AnnotationUnion,
                    Rule::NonPEP604AnnotationOptional,
                ])
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn datetime_utc_alias_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP017.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY311,
                ..settings::LinterSettings::for_rule(Rule::DatetimeTimezoneUTC)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }

    #[test]
    fn unpack_pep_646_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP044.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY311,
                ..settings::LinterSettings::for_rule(Rule::NonPEP646Unpack)
            },
        )?;
        assert_messages!(diagnostics);
        Ok(())
    }
}
