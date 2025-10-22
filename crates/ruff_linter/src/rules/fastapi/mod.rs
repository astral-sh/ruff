//! FastAPI-specific rules.
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use ruff_python_ast::PythonVersion;

    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;
    use crate::{assert_diagnostics, assert_diagnostics_diff};

    #[test_case(Rule::FastApiRedundantResponseModel, Path::new("FAST001.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_0.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_1.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_2.py"))]
    #[test_case(Rule::FastApiUnusedPathParameter, Path::new("FAST003.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("fastapi").join(path).as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::FastApiRedundantResponseModel, Path::new("FAST001.py"))]
    #[test_case(Rule::FastApiUnusedPathParameter, Path::new("FAST003.py"))]
    fn deferred_annotations_diff(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!(
            "deferred_annotations_diff_{}_{}",
            rule_code.name(),
            path.to_string_lossy()
        );
        assert_diagnostics_diff!(
            snapshot,
            Path::new("fastapi").join(path).as_path(),
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY313.into(),
                ..LinterSettings::for_rule(rule_code)
            },
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY314.into(),
                ..LinterSettings::for_rule(rule_code)
            },
        );
        Ok(())
    }

    // FAST002 autofixes use `typing_extensions` on Python 3.8,
    // since `typing.Annotated` was added in Python 3.9
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_0.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_1.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_2.py"))]
    fn rules_py38(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}_py38", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("fastapi").join(path).as_path(),
            &LinterSettings {
                unresolved_target_version: PythonVersion::PY38.into(),
                ..LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
