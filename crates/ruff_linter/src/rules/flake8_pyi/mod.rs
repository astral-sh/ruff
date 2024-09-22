//! Rules from [flake8-pyi](https://pypi.org/project/flake8-pyi/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::rules::pep8_naming;
    use crate::settings::types::{PreviewMode, PythonVersion};
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::AnyEqNeAnnotation, Path::new("PYI032.py"))]
    #[test_case(Rule::AnyEqNeAnnotation, Path::new("PYI032.pyi"))]
    #[test_case(Rule::ArgumentDefaultInStub, Path::new("PYI014.py"))]
    #[test_case(Rule::ArgumentDefaultInStub, Path::new("PYI014.pyi"))]
    #[test_case(Rule::AssignmentDefaultInStub, Path::new("PYI015.py"))]
    #[test_case(Rule::AssignmentDefaultInStub, Path::new("PYI015.pyi"))]
    #[test_case(Rule::BadExitAnnotation, Path::new("PYI036.py"))]
    #[test_case(Rule::BadExitAnnotation, Path::new("PYI036.pyi"))]
    #[test_case(Rule::BadVersionInfoComparison, Path::new("PYI006.py"))]
    #[test_case(Rule::BadVersionInfoComparison, Path::new("PYI006.pyi"))]
    #[test_case(Rule::BadVersionInfoOrder, Path::new("PYI066.py"))]
    #[test_case(Rule::BadVersionInfoOrder, Path::new("PYI066.pyi"))]
    #[test_case(Rule::ByteStringUsage, Path::new("PYI057.py"))]
    #[test_case(Rule::ByteStringUsage, Path::new("PYI057.pyi"))]
    #[test_case(Rule::CollectionsNamedTuple, Path::new("PYI024.py"))]
    #[test_case(Rule::CollectionsNamedTuple, Path::new("PYI024.pyi"))]
    #[test_case(Rule::ComplexAssignmentInStub, Path::new("PYI017.py"))]
    #[test_case(Rule::ComplexAssignmentInStub, Path::new("PYI017.pyi"))]
    #[test_case(Rule::ComplexIfStatementInStub, Path::new("PYI002.py"))]
    #[test_case(Rule::ComplexIfStatementInStub, Path::new("PYI002.pyi"))]
    #[test_case(Rule::DocstringInStub, Path::new("PYI021.py"))]
    #[test_case(Rule::DocstringInStub, Path::new("PYI021.pyi"))]
    #[test_case(Rule::DuplicateLiteralMember, Path::new("PYI062.py"))]
    #[test_case(Rule::DuplicateLiteralMember, Path::new("PYI062.pyi"))]
    #[test_case(Rule::DuplicateUnionMember, Path::new("PYI016.py"))]
    #[test_case(Rule::DuplicateUnionMember, Path::new("PYI016.pyi"))]
    #[test_case(Rule::EllipsisInNonEmptyClassBody, Path::new("PYI013.py"))]
    #[test_case(Rule::EllipsisInNonEmptyClassBody, Path::new("PYI013.pyi"))]
    #[test_case(Rule::FutureAnnotationsInStub, Path::new("PYI044.py"))]
    #[test_case(Rule::FutureAnnotationsInStub, Path::new("PYI044.pyi"))]
    #[test_case(Rule::GeneratorReturnFromIterMethod, Path::new("PYI058.py"))]
    #[test_case(Rule::GeneratorReturnFromIterMethod, Path::new("PYI058.pyi"))]
    #[test_case(Rule::GenericNotLastBaseClass, Path::new("PYI059.py"))]
    #[test_case(Rule::GenericNotLastBaseClass, Path::new("PYI059.pyi"))]
    #[test_case(Rule::IterMethodReturnIterable, Path::new("PYI045.py"))]
    #[test_case(Rule::IterMethodReturnIterable, Path::new("PYI045.pyi"))]
    #[test_case(Rule::NoReturnArgumentAnnotationInStub, Path::new("PYI050.py"))]
    #[test_case(Rule::NoReturnArgumentAnnotationInStub, Path::new("PYI050.pyi"))]
    #[test_case(Rule::NonEmptyStubBody, Path::new("PYI010.py"))]
    #[test_case(Rule::NonEmptyStubBody, Path::new("PYI010.pyi"))]
    #[test_case(Rule::NonSelfReturnType, Path::new("PYI034.py"))]
    #[test_case(Rule::NonSelfReturnType, Path::new("PYI034.pyi"))]
    #[test_case(Rule::NumericLiteralTooLong, Path::new("PYI054.py"))]
    #[test_case(Rule::NumericLiteralTooLong, Path::new("PYI054.pyi"))]
    #[test_case(Rule::PassInClassBody, Path::new("PYI012.py"))]
    #[test_case(Rule::PassInClassBody, Path::new("PYI012.pyi"))]
    #[test_case(Rule::PassStatementStubBody, Path::new("PYI009.py"))]
    #[test_case(Rule::PassStatementStubBody, Path::new("PYI009.pyi"))]
    #[test_case(Rule::PatchVersionComparison, Path::new("PYI004.py"))]
    #[test_case(Rule::PatchVersionComparison, Path::new("PYI004.pyi"))]
    #[test_case(Rule::QuotedAnnotationInStub, Path::new("PYI020.py"))]
    #[test_case(Rule::QuotedAnnotationInStub, Path::new("PYI020.pyi"))]
    #[test_case(Rule::PrePep570PositionalArgument, Path::new("PYI063.py"))]
    #[test_case(Rule::PrePep570PositionalArgument, Path::new("PYI063.pyi"))]
    #[test_case(Rule::RedundantFinalLiteral, Path::new("PYI064.py"))]
    #[test_case(Rule::RedundantFinalLiteral, Path::new("PYI064.pyi"))]
    #[test_case(Rule::RedundantLiteralUnion, Path::new("PYI051.py"))]
    #[test_case(Rule::RedundantLiteralUnion, Path::new("PYI051.pyi"))]
    #[test_case(Rule::RedundantNumericUnion, Path::new("PYI041.py"))]
    #[test_case(Rule::RedundantNumericUnion, Path::new("PYI041.pyi"))]
    #[test_case(Rule::SnakeCaseTypeAlias, Path::new("PYI042.py"))]
    #[test_case(Rule::SnakeCaseTypeAlias, Path::new("PYI042.pyi"))]
    #[test_case(Rule::StrOrReprDefinedInStub, Path::new("PYI029.py"))]
    #[test_case(Rule::StrOrReprDefinedInStub, Path::new("PYI029.pyi"))]
    #[test_case(Rule::StringOrBytesTooLong, Path::new("PYI053.py"))]
    #[test_case(Rule::StringOrBytesTooLong, Path::new("PYI053.pyi"))]
    #[test_case(Rule::StubBodyMultipleStatements, Path::new("PYI048.py"))]
    #[test_case(Rule::StubBodyMultipleStatements, Path::new("PYI048.pyi"))]
    #[test_case(Rule::TSuffixedTypeAlias, Path::new("PYI043.py"))]
    #[test_case(Rule::TSuffixedTypeAlias, Path::new("PYI043.pyi"))]
    #[test_case(Rule::TypeAliasWithoutAnnotation, Path::new("PYI026.py"))]
    #[test_case(Rule::TypeAliasWithoutAnnotation, Path::new("PYI026.pyi"))]
    #[test_case(Rule::TypeCommentInStub, Path::new("PYI033.py"))]
    #[test_case(Rule::TypeCommentInStub, Path::new("PYI033.pyi"))]
    #[test_case(Rule::TypedArgumentDefaultInStub, Path::new("PYI011.py"))]
    #[test_case(Rule::TypedArgumentDefaultInStub, Path::new("PYI011.pyi"))]
    #[test_case(Rule::UnaliasedCollectionsAbcSetImport, Path::new("PYI025_1.py"))]
    #[test_case(Rule::UnaliasedCollectionsAbcSetImport, Path::new("PYI025_1.pyi"))]
    #[test_case(Rule::UnaliasedCollectionsAbcSetImport, Path::new("PYI025_2.py"))]
    #[test_case(Rule::UnaliasedCollectionsAbcSetImport, Path::new("PYI025_2.pyi"))]
    #[test_case(Rule::UnaliasedCollectionsAbcSetImport, Path::new("PYI025_3.py"))]
    #[test_case(Rule::UnaliasedCollectionsAbcSetImport, Path::new("PYI025_3.pyi"))]
    #[test_case(Rule::UnannotatedAssignmentInStub, Path::new("PYI052.py"))]
    #[test_case(Rule::UnannotatedAssignmentInStub, Path::new("PYI052.pyi"))]
    #[test_case(Rule::UnassignedSpecialVariableInStub, Path::new("PYI035.py"))]
    #[test_case(Rule::UnassignedSpecialVariableInStub, Path::new("PYI035.pyi"))]
    #[test_case(Rule::UnnecessaryLiteralUnion, Path::new("PYI030.py"))]
    #[test_case(Rule::UnnecessaryLiteralUnion, Path::new("PYI030.pyi"))]
    #[test_case(Rule::UnnecessaryTypeUnion, Path::new("PYI055.py"))]
    #[test_case(Rule::UnnecessaryTypeUnion, Path::new("PYI055.pyi"))]
    #[test_case(Rule::UnprefixedTypeParam, Path::new("PYI001.py"))]
    #[test_case(Rule::UnprefixedTypeParam, Path::new("PYI001.pyi"))]
    #[test_case(Rule::UnrecognizedPlatformCheck, Path::new("PYI007.py"))]
    #[test_case(Rule::UnrecognizedPlatformCheck, Path::new("PYI007.pyi"))]
    #[test_case(Rule::UnrecognizedPlatformName, Path::new("PYI008.py"))]
    #[test_case(Rule::UnrecognizedPlatformName, Path::new("PYI008.pyi"))]
    #[test_case(Rule::UnrecognizedVersionInfoCheck, Path::new("PYI003.py"))]
    #[test_case(Rule::UnrecognizedVersionInfoCheck, Path::new("PYI003.pyi"))]
    #[test_case(Rule::UnsupportedMethodCallOnAll, Path::new("PYI056.py"))]
    #[test_case(Rule::UnsupportedMethodCallOnAll, Path::new("PYI056.pyi"))]
    #[test_case(Rule::UnusedPrivateProtocol, Path::new("PYI046.py"))]
    #[test_case(Rule::UnusedPrivateProtocol, Path::new("PYI046.pyi"))]
    #[test_case(Rule::UnusedPrivateTypeAlias, Path::new("PYI047.py"))]
    #[test_case(Rule::UnusedPrivateTypeAlias, Path::new("PYI047.pyi"))]
    #[test_case(Rule::UnusedPrivateTypeVar, Path::new("PYI018.py"))]
    #[test_case(Rule::UnusedPrivateTypeVar, Path::new("PYI018.pyi"))]
    #[test_case(Rule::UnusedPrivateTypedDict, Path::new("PYI049.py"))]
    #[test_case(Rule::UnusedPrivateTypedDict, Path::new("PYI049.pyi"))]
    #[test_case(Rule::WrongTupleLengthVersionComparison, Path::new("PYI005.py"))]
    #[test_case(Rule::WrongTupleLengthVersionComparison, Path::new("PYI005.pyi"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_pyi").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::CustomTypeVarReturnType, Path::new("PYI019.py"))]
    #[test_case(Rule::CustomTypeVarReturnType, Path::new("PYI019.pyi"))]
    fn custom_classmethod_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_pyi").join(path).as_path(),
            &settings::LinterSettings {
                pep8_naming: pep8_naming::settings::Settings {
                    classmethod_decorators: vec!["foo_classmethod".to_string()],
                    ..pep8_naming::settings::Settings::default()
                },
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::TypeAliasWithoutAnnotation, Path::new("PYI026.py"))]
    #[test_case(Rule::TypeAliasWithoutAnnotation, Path::new("PYI026.pyi"))]
    fn py38(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("py38_{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_pyi").join(path).as_path(),
            &settings::LinterSettings {
                target_version: PythonVersion::Py38,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::FutureAnnotationsInStub, Path::new("PYI044.pyi"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "preview__{}_{}",
            rule_code.noqa_code(),
            path.to_string_lossy()
        );
        let diagnostics = test_path(
            Path::new("flake8_pyi").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
