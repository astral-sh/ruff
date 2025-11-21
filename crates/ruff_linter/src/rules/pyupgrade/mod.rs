//! Rules from [pyupgrade](https://pypi.org/project/pyupgrade/).
pub(crate) mod fixes;
mod helpers;
pub(crate) mod rules;
pub mod settings;
pub(crate) mod types;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;
    use ruff_python_ast::PythonVersion;
    use ruff_python_semantic::{MemberNameImport, NameImport};
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::{isort, pyupgrade};
    use crate::settings::types::PreviewMode;
    use crate::test::{test_path, test_snippet};
    use crate::{assert_diagnostics, assert_diagnostics_diff, settings};

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
    #[test_case(Rule::NativeLiterals, Path::new("UP018_CR.py"))]
    #[test_case(Rule::NativeLiterals, Path::new("UP018_LF.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_0.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_1.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_2.py"))]
    #[test_case(Rule::NonPEP585Annotation, Path::new("UP006_3.py"))]
    #[test_case(Rule::NonPEP604AnnotationUnion, Path::new("UP007.py"))]
    #[test_case(Rule::NonPEP604AnnotationOptional, Path::new("UP045.py"))]
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
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_3.py"))]
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
    #[test_case(Rule::UnnecessaryBuiltinImport, Path::new("UP029_0.py"))]
    #[test_case(Rule::UnnecessaryBuiltinImport, Path::new("UP029_2.py"))]
    #[test_case(Rule::UnnecessaryClassParentheses, Path::new("UP039.py"))]
    #[test_case(Rule::UnnecessaryDefaultTypeArgs, Path::new("UP043.py"))]
    #[test_case(Rule::UnnecessaryEncodeUTF8, Path::new("UP012.py"))]
    #[test_case(Rule::UnnecessaryFutureImport, Path::new("UP010_0.py"))]
    #[test_case(Rule::UnnecessaryFutureImport, Path::new("UP010_1.py"))]
    #[test_case(Rule::UselessMetaclassType, Path::new("UP001.py"))]
    #[test_case(Rule::UselessObjectInheritance, Path::new("UP004.py"))]
    #[test_case(Rule::YieldInForLoop, Path::new("UP028_0.py"))]
    #[test_case(Rule::YieldInForLoop, Path::new("UP028_1.py"))]
    #[test_case(Rule::NonPEP695TypeAlias, Path::new("UP040.py"))]
    #[test_case(Rule::NonPEP695TypeAlias, Path::new("UP040.pyi"))]
    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_0.py"))]
    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_1.py"))]
    #[test_case(Rule::NonPEP695GenericFunction, Path::new("UP047_0.py"))]
    #[test_case(Rule::PrivateTypeParameter, Path::new("UP049_0.py"))]
    #[test_case(Rule::PrivateTypeParameter, Path::new("UP049_1.py"))]
    #[test_case(Rule::UselessClassMetaclassType, Path::new("UP050.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().to_string();
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_2.py"))]
    #[test_case(Rule::NonPEP695GenericFunction, Path::new("UP047_1.py"))]
    fn rules_not_applied_default_typevar_backported(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().to_string();
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                unresolved_target_version: PythonVersion::PY312.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::SuperCallWithParameters, Path::new("UP008.py"))]
    #[test_case(Rule::TypingTextStrAlias, Path::new("UP019.py"))]
    fn rules_preview(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}__preview", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_3.py"))]
    fn rules_py313(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("rules_py313__{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY313.into(),
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::NonPEP695TypeAlias, Path::new("UP040.py"))]
    #[test_case(Rule::NonPEP695TypeAlias, Path::new("UP040.pyi"))]
    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_0.py"))]
    #[test_case(Rule::NonPEP695GenericClass, Path::new("UP046_1.py"))]
    #[test_case(Rule::NonPEP695GenericFunction, Path::new("UP047_0.py"))]
    fn type_var_default_preview(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}__preview_diff", path.to_string_lossy());
        assert_diagnostics_diff!(
            snapshot,
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Disabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        );
        Ok(())
    }

    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_0.py"))]
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_1.py"))]
    #[test_case(Rule::QuotedAnnotation, Path::new("UP037_2.pyi"))]
    fn up037_add_future_annotation(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("add_future_annotation_{}", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings {
                future_annotations: true,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn async_timeout_error_alias_not_applied_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP041.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY310.into(),
                ..settings::LinterSettings::for_rule(Rule::TimeoutErrorAlias)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn non_pep695_type_alias_not_applied_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP040.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY311.into(),
                ..settings::LinterSettings::for_rule(Rule::NonPEP695TypeAlias)
            },
        )?;
        assert_diagnostics!(diagnostics);
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
                unresolved_target_version: PythonVersion::PY37.into(),
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_diagnostics!(diagnostics);
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
                unresolved_target_version: PythonVersion::PY310.into(),
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY37.into(),
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_585_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY310.into(),
                ..settings::LinterSettings::for_rule(Rule::NonPEP585Annotation)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_p37() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY37.into(),
                ..settings::LinterSettings::for_rules([
                    Rule::NonPEP604AnnotationUnion,
                    Rule::NonPEP604AnnotationOptional,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn future_annotations_pep_604_py310() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/future_annotations.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY310.into(),
                ..settings::LinterSettings::for_rules([
                    Rule::NonPEP604AnnotationUnion,
                    Rule::NonPEP604AnnotationOptional,
                ])
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn datetime_utc_alias_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP017.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY311.into(),
                ..settings::LinterSettings::for_rule(Rule::DatetimeTimezoneUTC)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn unpack_pep_646_py311() -> Result<()> {
        let diagnostics = test_path(
            Path::new("pyupgrade/UP044.py"),
            &settings::LinterSettings {
                unresolved_target_version: PythonVersion::PY311.into(),
                ..settings::LinterSettings::for_rule(Rule::NonPEP646Unpack)
            },
        )?;
        assert_diagnostics!(diagnostics);
        Ok(())
    }

    #[test]
    fn i002_conflict() {
        let diagnostics = test_snippet(
            "from pipes import quote, Template",
            &settings::LinterSettings {
                isort: isort::settings::Settings {
                    required_imports: BTreeSet::from_iter([
                        // https://github.com/astral-sh/ruff/issues/18729
                        NameImport::ImportFrom(MemberNameImport::member(
                            "__future__".to_string(),
                            "generator_stop".to_string(),
                        )),
                        // https://github.com/astral-sh/ruff/issues/16802
                        NameImport::ImportFrom(MemberNameImport::member(
                            "collections".to_string(),
                            "Sequence".to_string(),
                        )),
                        // Only bail out if _all_ the names in UP035 are required. `pipes.Template`
                        // isn't flagged by UP035, so requiring it shouldn't prevent `pipes.quote`
                        // from getting a diagnostic.
                        NameImport::ImportFrom(MemberNameImport::member(
                            "pipes".to_string(),
                            "Template".to_string(),
                        )),
                    ]),
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rules([
                    Rule::MissingRequiredImport,
                    Rule::UnnecessaryFutureImport,
                    Rule::DeprecatedImport,
                ])
            },
        );
        assert_diagnostics!(diagnostics, @r"
        UP035 [*] Import from `shlex` instead: `quote`
         --> <filename>:1:1
          |
        1 | from pipes import quote, Template
          | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
          |
        help: Import from `shlex`
          - from pipes import quote, Template
        1 + from pipes import Template
        2 + from shlex import quote

        I002 [*] Missing required import: `from __future__ import generator_stop`
        --> <filename>:1:1
        help: Insert required import: `from __future__ import generator_stop`
        1 + from __future__ import generator_stop
        2 | from pipes import quote, Template

        I002 [*] Missing required import: `from collections import Sequence`
        --> <filename>:1:1
        help: Insert required import: `from collections import Sequence`
        1 + from collections import Sequence
        2 | from pipes import quote, Template
        ");
    }

    #[test_case(Path::new("UP029_1.py"))]
    fn i002_up029_conflict(path: &Path) -> Result<()> {
        let snapshot = format!("{}_skip_required_imports", path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("pyupgrade").join(path).as_path(),
            &settings::LinterSettings {
                isort: isort::settings::Settings {
                    required_imports: BTreeSet::from_iter([
                        // https://github.com/astral-sh/ruff/issues/20601
                        NameImport::ImportFrom(MemberNameImport::member(
                            "builtins".to_string(),
                            "str".to_string(),
                        )),
                    ]),
                    ..Default::default()
                },
                ..settings::LinterSettings::for_rules([
                    Rule::MissingRequiredImport,
                    Rule::UnnecessaryBuiltinImport,
                ])
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn unnecessary_default_type_args_stubs_py312_preview() -> Result<()> {
        let snapshot = format!("{}__preview", "UP043.pyi");
        let diagnostics = test_path(
            Path::new("pyupgrade/UP043.pyi"),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                unresolved_target_version: PythonVersion::PY312.into(),
                ..settings::LinterSettings::for_rule(Rule::UnnecessaryDefaultTypeArgs)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
