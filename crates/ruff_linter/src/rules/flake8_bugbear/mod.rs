//! Rules from [flake8-bugbear](https://pypi.org/project/flake8-bugbear/).
pub(crate) mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use ruff_db::diagnostic::DiagnosticTag;
    use test_case::test_case;

    use crate::assert_diagnostics;
    use crate::registry::Rule;

    use crate::settings::LinterSettings;
    use crate::test::{test_path, test_snippet};

    use ruff_python_ast::PythonVersion;

    #[test_case(Rule::AbstractBaseClassWithoutAbstractMethod, Path::new("B024.py"))]
    #[test_case(Rule::AssertFalse, Path::new("B011.py"))]
    #[test_case(Rule::AssertRaisesException, Path::new("B017_0.py"))]
    #[test_case(Rule::AssertRaisesException, Path::new("B017_1.py"))]
    #[test_case(Rule::AssignmentToOsEnviron, Path::new("B003.py"))]
    #[test_case(Rule::CachedInstanceMethod, Path::new("B019.py"))]
    #[test_case(Rule::ClassAsDataStructure, Path::new("class_as_data_structure.py"))]
    #[test_case(Rule::DuplicateHandlerException, Path::new("B014.py"))]
    #[test_case(Rule::DuplicateTryBlockException, Path::new("B025.py"))]
    #[test_case(Rule::DuplicateValue, Path::new("B033.py"))]
    #[test_case(Rule::EmptyMethodWithoutAbstractDecorator, Path::new("B027.py"))]
    #[test_case(Rule::EmptyMethodWithoutAbstractDecorator, Path::new("B027.pyi"))]
    #[test_case(Rule::ExceptWithEmptyTuple, Path::new("B029.py"))]
    #[test_case(Rule::ExceptWithNonExceptionClasses, Path::new("B030.py"))]
    #[test_case(Rule::FStringDocstring, Path::new("B021.py"))]
    #[test_case(Rule::FunctionCallInDefaultArgument, Path::new("B006_B008.py"))]
    #[test_case(Rule::FunctionUsesLoopVariable, Path::new("B023.py"))]
    #[test_case(Rule::GetAttrWithConstant, Path::new("B009_B010.py"))]
    #[test_case(Rule::JumpStatementInFinally, Path::new("B012.py"))]
    #[test_case(Rule::LoopVariableOverridesIterator, Path::new("B020.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_1.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_2.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_3.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_4.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_5.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_6.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_7.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_8.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_9.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_B008.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_1.pyi"))]
    #[test_case(Rule::NoExplicitStacklevel, Path::new("B028.py"))]
    #[test_case(Rule::RaiseLiteral, Path::new("B016.py"))]
    #[test_case(Rule::RaiseWithoutFromInsideExcept, Path::new("B904.py"))]
    #[test_case(Rule::ReSubPositionalArgs, Path::new("B034.py"))]
    #[test_case(Rule::RedundantTupleInExceptionHandler, Path::new("B013.py"))]
    #[test_case(Rule::ReuseOfGroupbyGenerator, Path::new("B031.py"))]
    #[test_case(Rule::DelAttrWithConstant, Path::new("B043.py"))]
    #[test_case(Rule::SetAttrWithConstant, Path::new("B009_B010.py"))]
    #[test_case(Rule::StarArgUnpackingAfterKeywordArg, Path::new("B026.py"))]
    #[test_case(Rule::StaticKeyDictComprehension, Path::new("B035.py"))]
    #[test_case(Rule::StripWithMultiCharacters, Path::new("B005.py"))]
    #[test_case(Rule::UnaryPrefixIncrementDecrement, Path::new("B002.py"))]
    #[test_case(Rule::UnintentionalTypeAnnotation, Path::new("B032.py"))]
    #[test_case(Rule::UnreliableCallableCheck, Path::new("B004.py"))]
    #[test_case(Rule::UnusedLoopControlVariable, Path::new("B007.py"))]
    #[test_case(Rule::UselessComparison, Path::new("B015.ipynb"))]
    #[test_case(Rule::UselessComparison, Path::new("B015.py"))]
    #[test_case(Rule::UselessContextlibSuppress, Path::new("B022.py"))]
    #[test_case(Rule::UselessExpression, Path::new("B018.ipynb"))]
    #[test_case(Rule::UselessExpression, Path::new("B018.py"))]
    #[test_case(Rule::ReturnInGenerator, Path::new("B901.py"))]
    #[test_case(Rule::LoopIteratorMutation, Path::new("B909.py"))]
    #[test_case(Rule::MutableContextvarDefault, Path::new("B039.py"))]
    #[test_case(Rule::BatchedWithoutExplicitStrict, Path::new("B911.py"))]
    #[test_case(Rule::MapWithoutExplicitStrict, Path::new("B912.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_1.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_2.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_3.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_4.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_5.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_6.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_7.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_8.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_9.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_B008.py"))]
    #[test_case(Rule::MutableArgumentDefault, Path::new("B006_1.pyi"))]
    #[test_case(Rule::StripWithMultiCharacters, Path::new("B005.py"))]
    #[test_case(Rule::DuplicateHandlerException, Path::new("B014.py"))]
    #[test_case(Rule::DuplicateTryBlockException, Path::new("B025.py"))]
    fn preview_rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("preview__{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(path).as_path(),
            &LinterSettings::for_rule(rule_code)
                .with_preview_mode()
                .with_target_version(PythonVersion::PY314),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(false)]
    #[test_case(true)]
    fn duplicate_exceptions_together(preview: bool) -> Result<()> {
        let mut settings = LinterSettings::for_rules([
            Rule::DuplicateHandlerException,
            Rule::DuplicateTryBlockException,
        ]);
        if preview {
            settings = settings.with_preview_mode();
        }
        let diagnostics = test_path(Path::new("flake8_bugbear/B025.py"), &settings)?;
        assert_diagnostics!(
            format!("duplicate_exceptions_together_{preview}"),
            diagnostics
        );
        Ok(())
    }

    #[test]
    fn duplicate_exceptions_together_deferred() {
        let source = r"
def handle():
    try:
        ...
    except (OSError, TimeoutError):
        pass
    except TimeoutError:
        pass
";
        let baseline = test_snippet(
            source,
            &LinterSettings::for_rule(Rule::DuplicateTryBlockException).with_preview_mode(),
        );
        let diagnostics = test_snippet(
            source,
            &LinterSettings::for_rules([
                Rule::DuplicateHandlerException,
                Rule::DuplicateTryBlockException,
            ])
            .with_preview_mode(),
        );
        assert_eq!(baseline.len(), 1);
        assert_eq!(diagnostics.len(), 2);
        let expected: Vec<_> = baseline
            .iter()
            .map(|diagnostic| (diagnostic.headline_message(), diagnostic.range()))
            .collect();
        let actual: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.secondary_code_or_id() == "B025")
            .map(|diagnostic| (diagnostic.headline_message(), diagnostic.range()))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test_case(PythonVersion::PY39, 0)]
    #[test_case(PythonVersion::PY310, 1)]
    #[test_case(PythonVersion::PY313, 2)]
    #[test_case(PythonVersion::PY315, 3)]
    fn duplicate_handler_exception_target_version(version: PythonVersion, expected: usize) {
        let settings = LinterSettings::for_rule(Rule::DuplicateHandlerException)
            .with_preview_mode()
            .with_target_version(version);
        let diagnostics = test_snippet(
            r"
from builtins import EncodingWarning, ImportCycleError, PythonFinalizationError

try:
    pass
except (Warning, EncodingWarning):
    pass

try:
    pass
except (RuntimeError, PythonFinalizationError):
    pass

try:
    pass
except (ImportError, ImportCycleError):
    pass
",
            &settings,
        );
        assert_eq!(diagnostics.len(), expected);
    }

    #[test_case("TimeoutError")]
    #[test_case("OSError")]
    fn duplicate_handler_exception_shadowed_later(name: &str) {
        let settings =
            LinterSettings::for_rule(Rule::DuplicateHandlerException).with_preview_mode();
        let diagnostics = test_snippet(
            &format!(
                r"
class MyError(Exception):
    pass

def shadowed_later():
    try:
        pass
    except (OSError, TimeoutError):
        pass

    {name} = MyError
"
            ),
            &settings,
        );
        assert!(diagnostics.is_empty());
    }

    #[test_case("TimeoutError")]
    #[test_case("OSError")]
    fn duplicate_handler_exception_rebound_after_handler(name: &str) {
        let settings =
            LinterSettings::for_rule(Rule::DuplicateHandlerException).with_preview_mode();
        let diagnostics = test_snippet(
            &format!(
                r"
class MyError(Exception):
    pass

def rebound_after_handler():
    from builtins import {name}

    try:
        pass
    except (OSError, TimeoutError):
        {name} = MyError
"
            ),
            &settings,
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test_case("", "", "TimeoutError")]
    #[test_case("", "", "OSError")]
    #[test_case("class ShadowedLater:", "    ", "TimeoutError")]
    #[test_case("class ShadowedLater:", "    ", "OSError")]
    fn duplicate_handler_exception_shadowed_later_sequential_scope(
        scope: &str,
        indent: &str,
        name: &str,
    ) {
        let settings =
            LinterSettings::for_rule(Rule::DuplicateHandlerException).with_preview_mode();
        let diagnostics = test_snippet(
            &format!(
                r"
class MyError(Exception):
    pass

{scope}
{indent}try:
{indent}    pass
{indent}except (OSError, TimeoutError):
{indent}    pass

{indent}{name} = MyError
"
            ),
            &settings,
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test_case("TimeoutError")]
    #[test_case("OSError")]
    fn duplicate_handler_exception_nested_class_shadowed_later(name: &str) {
        let settings =
            LinterSettings::for_rule(Rule::DuplicateHandlerException).with_preview_mode();
        let diagnostics = test_snippet(
            &format!(
                r"
class MyError(Exception):
    pass

def outer():
    class Inner:
        try:
            ...
        except (OSError, TimeoutError):
            pass

    {name} = MyError
"
            ),
            &settings,
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn duplicate_handler_exception_nested_class_sequential_scope() {
        let settings =
            LinterSettings::for_rule(Rule::DuplicateHandlerException).with_preview_mode();
        let diagnostics = test_snippet(
            r"
class MyError(Exception):
    pass

def outer():
    class Inner:
        try:
            ...
        except (OSError, TimeoutError):
            pass

        TimeoutError = MyError
",
            &settings,
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn duplicate_handler_exception_nested_class_initialized_import() {
        let settings =
            LinterSettings::for_rule(Rule::DuplicateHandlerException).with_preview_mode();
        let diagnostics = test_snippet(
            r"
class MyError(Exception):
    pass

def outer():
    from builtins import TimeoutError

    class Inner:
        try:
            ...
        except (OSError, TimeoutError):
            pass

    TimeoutError = MyError
",
            &settings,
        );
        assert_eq!(diagnostics.len(), 1);
    }

    #[test_case(
        Rule::ClassAsDataStructure,
        Path::new("class_as_data_structure.py"),
        PythonVersion::PY39
    )]
    #[test_case(
        Rule::MapWithoutExplicitStrict,
        Path::new("B912.py"),
        PythonVersion::PY313
    )]
    #[test_case(
        Rule::StaticKeyDictComprehension,
        Path::new("B035_py315.py"),
        PythonVersion::PY315
    )]
    fn rules_with_target_version(
        rule_code: Rule,
        path: &Path,
        target_version: PythonVersion,
    ) -> Result<()> {
        let snapshot = format!(
            "{}_py{}{}_{}",
            rule_code.name(),
            target_version.major,
            target_version.minor,
            path.to_string_lossy(),
        );
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(path).as_path(),
            &LinterSettings::for_rule(rule_code).with_target_version(target_version),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn zip_without_explicit_strict() -> Result<()> {
        let snapshot = "B905.py";
        let diagnostics = test_path(
            Path::new("flake8_bugbear").join(snapshot).as_path(),
            &LinterSettings::for_rule(Rule::ZipWithoutExplicitStrict),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_immutable_calls_arg_annotation() -> Result<()> {
        let snapshot = "extend_immutable_calls_arg_annotation".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B006_extended.py"),
            &LinterSettings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec![
                        "custom.ImmutableTypeA".to_string(),
                        "custom.ImmutableTypeB".to_string(),
                    ],
                },
                ..LinterSettings::for_rule(Rule::MutableArgumentDefault)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_immutable_calls_arg_default() -> Result<()> {
        let snapshot = "extend_immutable_calls_arg_default".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B008_extended.py"),
            &LinterSettings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec![
                        "fastapi.Depends".to_string(),
                        "fastapi.Query".to_string(),
                        "custom.ImmutableTypeA".to_string(),
                        "B008_extended.Class".to_string(),
                    ],
                },
                ..LinterSettings::for_rule(Rule::FunctionCallInDefaultArgument)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn extend_mutable_contextvar_default() -> Result<()> {
        let snapshot = "extend_mutable_contextvar_default".to_string();
        let diagnostics = test_path(
            Path::new("flake8_bugbear/B039_extended.py"),
            &LinterSettings {
                flake8_bugbear: super::settings::Settings {
                    extend_immutable_calls: vec!["fastapi.Query".to_string()],
                },
                ..LinterSettings::for_rule(Rule::MutableContextvarDefault)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn b007_unnecessary_tag_only_for_certain_cases() {
        let settings = LinterSettings::for_rule(Rule::UnusedLoopControlVariable);

        let certain = test_snippet(
            r"
for i in range(3):
    print(1)
",
            &settings,
        );
        assert_eq!(certain.len(), 1);
        assert!(
            certain[0]
                .primary_tags()
                .is_some_and(|tags| tags.contains(&DiagnosticTag::Unnecessary))
        );

        let uncertain = test_snippet(
            r"
for i in range(3):
    print(locals())
",
            &settings,
        );
        assert_eq!(uncertain.len(), 1);
        assert!(
            !uncertain[0]
                .primary_tags()
                .is_some_and(|tags| tags.contains(&DiagnosticTag::Unnecessary))
        );
    }
}
